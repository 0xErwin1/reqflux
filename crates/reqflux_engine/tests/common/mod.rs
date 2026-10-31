//! Helpers shared by the engine integration tests.

#![allow(dead_code, unreachable_pub)]
#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use std::fs;
use std::path::Path;

use reqflux_core::model::{
    Auth, Collection, CollectionNode, Environment, EnvironmentNode, Folder, FolderNode, Id, Item,
    KeyValue, Method, Request, RequestNode, Scripts, Variable, Workspace, WorkspaceManifest,
};
use reqflux_core::template::SequentialDynamicSource;
use reqflux_engine::{ExecuteOptions, Prepared};
use tempfile::TempDir;

/// The workspace root the in-memory tests prepare against.
pub const ROOT: &str = "/workspace";

/// A ULID made of a fixed prefix and `n`, so tests can name ids.
pub fn id_text(n: u32) -> String {
    format!("01J{n:023}")
}

pub fn id(n: u32) -> Id {
    match id_text(n).parse() {
        Ok(id) => id,
        Err(error) => panic!("invalid test id: {error}"),
    }
}

pub fn workspace(globals: Vec<Variable>, collections: Vec<CollectionNode>) -> Workspace {
    Workspace {
        manifest: WorkspaceManifest {
            id: id(900),
            name: "W".to_owned(),
            format: 1,
            globals,
            mcp: None,
            extensions: toml::Table::new(),
        },
        environments: Vec::new(),
        collections,
    }
}

pub fn environment(n: u32, slug: &str, variables: Vec<Variable>) -> EnvironmentNode {
    EnvironmentNode {
        slug: slug.to_owned(),
        environment: Environment {
            id: id(n),
            name: slug.to_owned(),
            variables,
            extensions: toml::Table::new(),
        },
    }
}

pub fn collection(slug: &str, items: Vec<Item>) -> CollectionNode {
    CollectionNode {
        slug: slug.to_owned(),
        collection: Collection {
            id: Id::new(),
            name: format!("{slug} name"),
            environments: Vec::new(),
            order: Vec::new(),
            auth: Auth::Inherit,
            variables: Vec::new(),
            extensions: toml::Table::new(),
        },
        scripts: Scripts::default(),
        items,
    }
}

pub fn folder(slug: &str, auth: Auth, items: Vec<Item>) -> Item {
    Item::Folder(FolderNode {
        slug: slug.to_owned(),
        folder: Folder {
            id: Id::new(),
            name: slug.to_owned(),
            order: Vec::new(),
            auth,
            extensions: toml::Table::new(),
        },
        scripts: Scripts::default(),
        items,
    })
}

pub fn request(slug: &str, request: Request) -> Item {
    Item::Request(RequestNode {
        slug: slug.to_owned(),
        request,
        scripts: Scripts::default(),
    })
}

pub fn get(url: &str) -> Request {
    Request::new(Id::new(), "R", Method::GET, url)
}

pub fn post(url: &str) -> Request {
    Request::new(Id::new(), "R", Method::POST, url)
}

pub fn disabled(key: &str, value: &str) -> KeyValue {
    KeyValue {
        enabled: false,
        ..KeyValue::new(key, value)
    }
}

/// A workspace with one collection `c` holding the request `c/r`.
pub fn single(request_doc: Request) -> Workspace {
    workspace(
        Vec::new(),
        vec![collection("c", vec![request("r", request_doc)])],
    )
}

pub fn options(environment: Option<&str>, vars: &[(&str, &str)]) -> ExecuteOptions {
    ExecuteOptions {
        environment: environment.map(str::to_owned),
        vars: vars
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        ..ExecuteOptions::default()
    }
}

pub fn prepare_with(workspace: &Workspace, selector: &str, options: &ExecuteOptions) -> Prepared {
    match reqflux_engine::prepare(
        workspace,
        Path::new(ROOT),
        selector,
        options,
        &mut SequentialDynamicSource::default(),
    ) {
        Ok(prepared) => prepared,
        Err(error) => panic!("prepare failed: {error}"),
    }
}

pub fn prepare(workspace: &Workspace) -> Prepared {
    prepare_with(workspace, "c/r", &ExecuteOptions::default())
}

pub fn prepare_err(workspace: &Workspace, selector: &str, options: &ExecuteOptions) -> String {
    match reqflux_engine::prepare(
        workspace,
        Path::new(ROOT),
        selector,
        options,
        &mut SequentialDynamicSource::default(),
    ) {
        Ok(prepared) => panic!("prepare unexpectedly succeeded: {prepared:?}"),
        Err(error) => error.to_string(),
    }
}

pub fn prepare_draft_with(
    workspace: &Workspace,
    selector: &str,
    draft: &Request,
    options: &ExecuteOptions,
) -> Prepared {
    match reqflux_engine::prepare_draft(
        workspace,
        Path::new(ROOT),
        selector,
        draft,
        options,
        &mut SequentialDynamicSource::default(),
    ) {
        Ok(prepared) => prepared,
        Err(error) => panic!("prepare draft failed: {error}"),
    }
}

pub fn prepare_draft_err(
    workspace: &Workspace,
    selector: &str,
    draft: &Request,
    options: &ExecuteOptions,
) -> String {
    match reqflux_engine::prepare_draft(
        workspace,
        Path::new(ROOT),
        selector,
        draft,
        options,
        &mut SequentialDynamicSource::default(),
    ) {
        Ok(prepared) => panic!("prepare draft unexpectedly succeeded: {prepared:?}"),
        Err(error) => error.to_string(),
    }
}

pub fn temp_dir() -> TempDir {
    match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

/// Every file under `root` with its bytes, in path order, so a test can prove a run left
/// the workspace untouched.
pub fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => panic!("read {}: {error}", dir.display()),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => panic!("list {}: {error}", dir.display()),
        };
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => panic!("read {}: {error}", path.display()),
            };
            out.push((
                path.strip_prefix(root).map_or_else(
                    |_| path.to_string_lossy().into_owned(),
                    |relative| relative.to_string_lossy().into_owned(),
                ),
                bytes,
            ));
        }
    }
}

pub fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create {}: {error}", parent.display());
    }
    if let Err(error) = fs::write(&path, content) {
        panic!("write {}: {error}", path.display());
    }
}

/// Opens the workspace at `root` with an empty in-memory secret store, so no test touches
/// the OS keyring (keyring-secrets S8).
pub fn open_engine(
    root: impl Into<std::path::PathBuf>,
) -> Result<reqflux_engine::Engine, reqflux_engine::OpenError> {
    reqflux_engine::Engine::open_with_secrets(
        root,
        std::sync::Arc::new(reqflux_engine::MemorySecretStore::new()),
    )
}
