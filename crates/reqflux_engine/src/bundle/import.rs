//! Importing a bundle (v1-features S6) with the Postman import's rule: every target file is
//! checked first, and an existing one fails the import before anything is written.
//!
//! A workspace bundle needs a directory without a `reqflux.toml` and becomes a new workspace
//! with a new id, so its secrets never land on the keyring entries of the workspace it came
//! from. A collection or environment bundle goes into the workspace, which is created, named
//! after it, when the directory has none. A folder or request bundle goes into the collection
//! or folder `into` names, at the end of it: the parent's file is not rewritten, so the
//! child is listed after the ordered ones. An id the target workspace already uses is
//! replaced by a new one. Encrypted secrets are decrypted, before anything is written, only
//! when a passphrase is given; they are then stored for the entities they belong to.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqflux_core::model::{
    Collection, CollectionNode, Environment, EnvironmentNode, Example, Folder, FolderNode, Id,
    Item, Request, RequestNode, WorkspaceManifest,
};
use reqflux_store::{
    Loaded, Parent, ScriptHashes, ScriptOwner, checked_collection_file, checked_environment_file,
    checked_folder_file, checked_request_file, create_example_at, example_files_at, manifest_file,
    save_collection, save_environment, save_folder, save_manifest, save_request, save_scripts,
};
use serde::de::DeserializeOwned;

use super::crypt;
use super::format::{
    BundleKind, CollectionEntry, Document, FORMAT, FORMAT_VERSION, Header, ItemEntry, RequestEntry,
    SecretList,
};
use super::scripts;
use crate::error::BundleError;
use crate::export::unit::{self, Unit};
use crate::secrets::{SecretKey, SecretScope, SecretStore};

/// How to import a bundle.
#[derive(Clone, Copy, Default)]
pub struct BundleImport<'a> {
    /// The id or slug path of the collection or folder a folder or request bundle goes into.
    pub into: Option<&'a str>,
    /// The passphrase of the encrypted secrets; without it they are skipped.
    pub passphrase: Option<&'a str>,
}

impl fmt::Debug for BundleImport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BundleImport")
            .field("into", &self.into)
            .field("passphrase", &self.passphrase.map(|_| "<given>"))
            .finish()
    }
}

/// What a bundle import wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSummary {
    /// The workspace root.
    pub root: PathBuf,
    /// What the bundle held.
    pub kind: BundleKind,
    /// Collections written.
    pub collections: usize,
    /// Folders written, at any depth.
    pub folders: usize,
    /// Requests written, at any depth.
    pub requests: usize,
    /// Saved examples written.
    pub examples: usize,
    /// Environments written.
    pub environments: usize,
    /// Entities that got a new id because the workspace already used theirs.
    pub renewed_ids: usize,
    /// Secret values stored.
    pub secrets_imported: usize,
    /// Whether the bundle carries encrypted secrets that were skipped for lack of a
    /// passphrase.
    pub secrets_skipped: bool,
    /// Why secret values were not stored after the files were written; the files stay.
    pub secret_problem: Option<String>,
}

/// Imports the bundle `text` into the workspace directory `root`, storing its decrypted
/// secrets in `store`.
///
/// # Errors
///
/// [`BundleError::Invalid`] or [`BundleError::UnsupportedFormat`] for a text that is not a
/// bundle this build reads; [`BundleError::Decrypt`] for a wrong passphrase;
/// [`BundleError::Parent`] for a missing, unknown or unexpected `into`;
/// [`BundleError::Exists`] when a target file exists; [`BundleError::Store`] when the
/// workspace does not load or a write fails. Nothing is written unless the checks pass.
pub fn import_bundle(
    root: &Path,
    text: &str,
    options: &BundleImport<'_>,
    store: &dyn SecretStore,
) -> Result<BundleSummary, BundleError> {
    let document = parse(text)?;
    let secrets = match (&document.secrets, options.passphrase) {
        (Some(section), Some(passphrase)) => {
            let plaintext = crypt::decrypt(passphrase, &section.armor)?;
            let list: SecretList = serde_json::from_slice(&plaintext).map_err(|_| {
                BundleError::Invalid("the secrets section does not parse".to_owned())
            })?;
            Some(list)
        }
        _ => None,
    };
    let manifest_path = manifest_file(root);
    let loaded = if manifest_path.exists() {
        if document.kind == BundleKind::Workspace {
            return Err(BundleError::Exists {
                path: manifest_path,
            });
        }
        Some(reqflux_store::load(root)?)
    } else {
        None
    };
    let parent = parent(&document, loaded.as_ref(), options.into)?;
    let mut ids = Ids::new(loaded.as_ref());
    let plan = plan(&document, &mut ids, parent.as_ref())?;
    let create_manifest = loaded.is_none();
    let mut targets = Vec::new();
    if create_manifest {
        targets.push(manifest_path.clone());
    }
    plan.targets(root, &mut targets)?;
    if let Some(existing) = targets.iter().find(|target| target.exists()) {
        return Err(BundleError::Exists {
            path: existing.clone(),
        });
    }

    let workspace_id = match (&loaded, &plan.manifest) {
        (Some(loaded), _) => loaded.workspace.manifest.id,
        (None, Some(manifest)) => manifest.id,
        (None, None) => Id::new(),
    };
    if create_manifest {
        let manifest = plan.manifest.clone().unwrap_or_else(|| WorkspaceManifest {
            id: workspace_id,
            name: document.name.clone(),
            format: reqflux_store::FORMAT,
            globals: Vec::new(),
            mcp: None,
            extensions: toml::Table::new(),
        });
        save_manifest(root, &manifest, None)?;
    }
    let mut summary = BundleSummary {
        root: root.to_owned(),
        kind: document.kind,
        collections: plan.collections.len(),
        folders: 0,
        requests: 0,
        examples: 0,
        environments: plan.environments.len(),
        renewed_ids: ids.renewed,
        secrets_imported: 0,
        secrets_skipped: document.secrets.is_some() && options.passphrase.is_none(),
        secret_problem: None,
    };
    plan.write(root, &mut summary)?;
    if let Some(list) = secrets {
        store_secrets(
            &list,
            workspace_id,
            &ids,
            document.kind,
            store,
            &mut summary,
        );
    }
    Ok(summary)
}

/// Parses the header first, so a newer format is refused before the rest is read. A parse
/// error names the line and column only: the text can hold literal credentials.
fn parse(text: &str) -> Result<Document, BundleError> {
    let invalid = |error: serde_json::Error| {
        BundleError::Invalid(format!(
            "line {}, column {}: {:?} error",
            error.line(),
            error.column(),
            error.classify()
        ))
    };
    let header: Header = serde_json::from_str(text).map_err(invalid)?;
    if header.format != FORMAT {
        return Err(BundleError::Invalid(format!(
            "`format` is not \"{FORMAT}\""
        )));
    }
    if header.format_version > FORMAT_VERSION {
        return Err(BundleError::UnsupportedFormat {
            found: header.format_version,
            supported: FORMAT_VERSION,
        });
    }
    serde_json::from_str(text).map_err(invalid)
}

/// The parent a folder or request bundle goes into; `None` for the other kinds, which
/// refuse one.
fn parent(
    document: &Document,
    loaded: Option<&Loaded>,
    into: Option<&str>,
) -> Result<Option<Parent>, BundleError> {
    let kind = document.kind.name();
    match (document.kind, into) {
        (BundleKind::Folder | BundleKind::Request, None) => Err(BundleError::Parent(format!(
            "a {kind} bundle needs a collection or folder to go into"
        ))),
        (BundleKind::Folder | BundleKind::Request, Some(into)) => {
            let unknown = || BundleError::Parent(format!("no collection or folder matches {into}"));
            let workspace = &loaded.ok_or_else(unknown)?.workspace;
            match unit::select(workspace, into) {
                Some(Unit::Collection(collection)) => {
                    Ok(Some(Parent::collection(&collection.slug)))
                }
                Some(Unit::Folder {
                    collection,
                    folders,
                }) => Ok(Some(Parent {
                    collection: collection.slug.clone(),
                    folders: folders.iter().map(|folder| folder.slug.clone()).collect(),
                })),
                _ => Err(unknown()),
            }
        }
        (_, Some(_)) => Err(BundleError::Parent(format!(
            "a {kind} bundle goes into the workspace, not into a collection or folder"
        ))),
        (_, None) => Ok(None),
    }
}

/// The ids the target workspace uses, the ones this import replaced and how many.
struct Ids {
    taken: BTreeSet<Id>,
    replaced: BTreeMap<Id, Id>,
    renewed: usize,
}

impl Ids {
    fn new(loaded: Option<&Loaded>) -> Self {
        let mut taken = BTreeSet::new();
        if let Some(loaded) = loaded {
            let workspace = &loaded.workspace;
            taken.insert(workspace.manifest.id);
            for environment in &workspace.environments {
                taken.insert(environment.environment.id);
            }
            for collection in &workspace.collections {
                taken.insert(collection.collection.id);
                collect_ids(&collection.items, &mut taken);
            }
        }
        Self {
            taken,
            replaced: BTreeMap::new(),
            renewed: 0,
        }
    }

    /// `id`, or a new id when it is taken; either way it is taken from now on.
    fn claim(&mut self, id: Id) -> Id {
        if self.taken.insert(id) {
            return id;
        }
        let fresh = Id::new();
        self.taken.insert(fresh);
        self.replaced.insert(id, fresh);
        self.renewed += 1;
        fresh
    }

    /// The id `id` became.
    fn current(&self, id: Id) -> Id {
        self.replaced.get(&id).copied().unwrap_or(id)
    }
}

fn collect_ids(items: &[Item], taken: &mut BTreeSet<Id>) {
    for item in items {
        match item {
            Item::Folder(folder) => {
                taken.insert(folder.folder.id);
                collect_ids(&folder.items, taken);
            }
            Item::Request(request) => {
                taken.insert(request.request.id);
            }
        }
    }
}

/// An example to write and its body file, if it had one.
type PlannedExample = (Example, Option<Vec<u8>>);

/// What the import writes.
struct Plan {
    manifest: Option<WorkspaceManifest>,
    environments: Vec<EnvironmentNode>,
    collections: Vec<CollectionNode>,
    /// A folder or request bundle's items and the parent they go into.
    items: Option<(Parent, Vec<Item>)>,
    examples: BTreeMap<Id, Vec<PlannedExample>>,
}

fn plan(document: &Document, ids: &mut Ids, parent: Option<&Parent>) -> Result<Plan, BundleError> {
    let manifest = match &document.manifest {
        Some(text) => {
            let mut manifest: WorkspaceManifest = parse_toml(text, "reqflux.toml")?;
            manifest.id = Id::new();
            ids.taken.insert(manifest.id);
            Some(manifest)
        }
        None => None,
    };
    let mut examples = BTreeMap::new();
    let mut environments = Vec::new();
    for entry in &document.environments {
        let mut environment: Environment = parse_toml(&entry.file, &entry.slug)?;
        environment.id = ids.claim(environment.id);
        environments.push(EnvironmentNode {
            slug: entry.slug.clone(),
            environment,
        });
    }
    let mut collections = Vec::new();
    for entry in &document.collections {
        let mut collection: Collection = parse_toml(&entry.file, &entry.slug)?;
        collection.id = ids.claim(collection.id);
        let items = items(&entry.items, ids, &mut examples)?;
        collections.push(CollectionNode {
            slug: entry.slug.clone(),
            collection,
            scripts: scripts(&entry.pre, &entry.test),
            items,
        });
    }
    // An allowlist names environments by id; follow the ones this import renewed.
    for collection in &mut collections {
        for id in &mut collection.collection.environments {
            *id = ids.current(*id);
        }
    }
    let items = match parent {
        Some(parent) => Some((parent.clone(), items(&document.items, ids, &mut examples)?)),
        None => None,
    };
    Ok(Plan {
        manifest,
        environments,
        collections,
        items,
        examples,
    })
}

fn items(
    entries: &[ItemEntry],
    ids: &mut Ids,
    examples: &mut BTreeMap<Id, Vec<PlannedExample>>,
) -> Result<Vec<Item>, BundleError> {
    entries
        .iter()
        .map(|entry| match entry {
            ItemEntry::Folder(entry) => folder(entry, ids, examples).map(Item::Folder),
            ItemEntry::Request(entry) => request(entry, ids, examples).map(Item::Request),
        })
        .collect()
}

fn folder(
    entry: &CollectionEntry,
    ids: &mut Ids,
    examples: &mut BTreeMap<Id, Vec<PlannedExample>>,
) -> Result<FolderNode, BundleError> {
    let mut folder: Folder = parse_toml(&entry.file, &entry.slug)?;
    folder.id = ids.claim(folder.id);
    Ok(FolderNode {
        slug: entry.slug.clone(),
        folder,
        scripts: scripts(&entry.pre, &entry.test),
        items: items(&entry.items, ids, examples)?,
    })
}

fn request(
    entry: &RequestEntry,
    ids: &mut Ids,
    examples: &mut BTreeMap<Id, Vec<PlannedExample>>,
) -> Result<RequestNode, BundleError> {
    let mut request: Request = parse_toml(&entry.file, &entry.slug)?;
    request.id = ids.claim(request.id);
    let mut planned = Vec::with_capacity(entry.examples.len());
    for example in &entry.examples {
        let parsed: Example = parse_toml(&example.file, &entry.slug)?;
        let body = match &example.body_base64 {
            Some(encoded) => Some(STANDARD.decode(encoded).map_err(|_| {
                BundleError::Invalid(format!("an example body of {} is not base64", entry.slug))
            })?),
            None => None,
        };
        planned.push((parsed, body));
    }
    if !planned.is_empty() {
        examples.insert(request.id, planned);
    }
    Ok(RequestNode {
        slug: entry.slug.clone(),
        request,
        scripts: scripts(&entry.pre, &entry.test),
    })
}

/// A TOML document of the bundle; an error names the entity and the line, never the text.
fn parse_toml<T: DeserializeOwned>(text: &str, name: &str) -> Result<T, BundleError> {
    toml::from_str(text).map_err(|error| {
        BundleError::Invalid(format!(
            "{name}: {}",
            reqflux_store::toml_error_location(text, &error)
        ))
    })
}

impl Plan {
    /// Every file the import writes, checked against the store's slug rules.
    fn targets(&self, root: &Path, targets: &mut Vec<PathBuf>) -> Result<(), BundleError> {
        for environment in &self.environments {
            targets.push(checked_environment_file(root, &environment.slug)?);
        }
        for collection in &self.collections {
            targets.push(checked_collection_file(root, &collection.slug)?);
            targets.extend(sidecars(
                &ScriptOwner::Collection(collection.slug.clone()),
                root,
            ));
            self.item_targets(
                root,
                &Parent::collection(&collection.slug),
                &collection.items,
                targets,
            )?;
        }
        if let Some((parent, items)) = &self.items {
            self.item_targets(root, parent, items, targets)?;
        }
        Ok(())
    }

    fn item_targets(
        &self,
        root: &Path,
        parent: &Parent,
        items: &[Item],
        targets: &mut Vec<PathBuf>,
    ) -> Result<(), BundleError> {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    targets.push(checked_folder_file(root, parent, &folder.slug)?);
                    targets.extend(sidecars(
                        &ScriptOwner::Folder(parent.clone(), folder.slug.clone()),
                        root,
                    ));
                    self.item_targets(root, &parent.child(&folder.slug), &folder.items, targets)?;
                }
                Item::Request(request) => {
                    targets.push(checked_request_file(root, parent, &request.slug)?);
                    targets.extend(sidecars(
                        &ScriptOwner::Request(parent.clone(), request.slug.clone()),
                        root,
                    ));
                    let names: Vec<&str> = self
                        .examples
                        .get(&request.request.id)
                        .into_iter()
                        .flatten()
                        .map(|(example, _)| example.name.as_str())
                        .collect();
                    targets.extend(example_files_at(root, parent, &request.slug, &names)?);
                }
            }
        }
        Ok(())
    }

    fn write(&self, root: &Path, summary: &mut BundleSummary) -> Result<(), BundleError> {
        for environment in &self.environments {
            save_environment(root, &environment.slug, &environment.environment, None)?;
        }
        for collection in &self.collections {
            save_collection(root, &collection.slug, &collection.collection, None)?;
            save_scripts(
                root,
                &ScriptOwner::Collection(collection.slug.clone()),
                &collection.scripts,
                ScriptHashes::default(),
            )?;
            self.write_items(
                root,
                &Parent::collection(&collection.slug),
                &collection.items,
                summary,
            )?;
        }
        if let Some((parent, items)) = &self.items {
            self.write_items(root, parent, items, summary)?;
        }
        Ok(())
    }

    fn write_items(
        &self,
        root: &Path,
        parent: &Parent,
        items: &[Item],
        summary: &mut BundleSummary,
    ) -> Result<(), BundleError> {
        for item in items {
            match item {
                Item::Folder(folder) => {
                    save_folder(root, parent, &folder.slug, &folder.folder, None)?;
                    save_scripts(
                        root,
                        &ScriptOwner::Folder(parent.clone(), folder.slug.clone()),
                        &folder.scripts,
                        ScriptHashes::default(),
                    )?;
                    summary.folders += 1;
                    self.write_items(root, &parent.child(&folder.slug), &folder.items, summary)?;
                }
                Item::Request(request) => {
                    save_request(root, parent, &request.slug, &request.request, None)?;
                    save_scripts(
                        root,
                        &ScriptOwner::Request(parent.clone(), request.slug.clone()),
                        &request.scripts,
                        ScriptHashes::default(),
                    )?;
                    summary.requests += 1;
                    for (example, body) in
                        self.examples.get(&request.request.id).into_iter().flatten()
                    {
                        create_example_at(
                            root,
                            parent,
                            &request.slug,
                            example.clone(),
                            body.as_deref(),
                        )?;
                        summary.examples += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

/// The two sidecar script paths of `owner`.
fn sidecars(owner: &ScriptOwner, root: &Path) -> [PathBuf; 2] {
    let files = owner.files(root);
    [files.pre_request, files.test]
}

/// Stores each decrypted secret under the target workspace and the entity it belongs to
/// now. A global secret goes in only with a workspace bundle; a scope the bundle does not
/// hold is skipped. The first store failure stops and is reported, the files staying.
fn store_secrets(
    list: &SecretList,
    workspace_id: Id,
    ids: &Ids,
    kind: BundleKind,
    store: &dyn SecretStore,
    summary: &mut BundleSummary,
) {
    for secret in &list.secrets {
        let scope = match SecretScope::from_path(&secret.scope) {
            Some(SecretScope::Global) if kind == BundleKind::Workspace => SecretScope::Global,
            Some(SecretScope::Collection(id)) => SecretScope::Collection(ids.current(id)),
            Some(SecretScope::Environment(id)) => SecretScope::Environment(ids.current(id)),
            _ => continue,
        };
        let key = SecretKey {
            workspace_id,
            scope,
            key: secret.key.clone(),
        };
        match store.set(&key, &secret.value) {
            Ok(()) => summary.secrets_imported += 1,
            Err(error) => {
                summary.secret_problem = Some(error.to_string());
                return;
            }
        }
    }
}
