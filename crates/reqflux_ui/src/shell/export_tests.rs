//! Export… in the window (v1-close S4, design/export/01-one-export-dialog): the dialog opens
//! from the tree menu, the workspace menu, an environment's menu and the palette with the
//! scope of where it opened; Postman v2.1 writes the engine's document, owner-only with a
//! plaintext warning when it carries secrets; the toasts say where and Show in folder reveals
//! the file; a failure offers Try again….

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use reqflux_engine::{Engine, SecretKey, SecretScope};
use tempfile::TempDir;

use super::{ExportHost, TestExportHost};
use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::toasts::tests::{click_node, error, pushed, success, tree_label, warning};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

pub(crate) const COLLECTION: &str = "01J000000000000000000000X1";
pub(crate) const FOLDER: &str = "01J000000000000000000000X2";
pub(crate) const CREATE: &str = "01J000000000000000000000X3";
pub(crate) const STAGING: &str = "01J000000000000000000000X4";

/// A workspace `Demo` with the collection `Billing API` (the secrets `access_token` and
/// `client_secret`), its folder `invoices` holding `POST Create invoice`, and the
/// environment `staging` with `host` and the secret `api_key`.
pub(crate) fn export_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/billing/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"Billing API\"\n\n\
             [[variables]]\nkey = \"base_url\"\nvalue = \"https://api.test\"\n\n\
             [[variables]]\nkey = \"access_token\"\nsecret = true\n\n\
             [[variables]]\nkey = \"client_secret\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/billing/invoices/folder.toml",
        &format!("id = \"{FOLDER}\"\nname = \"invoices\"\n"),
    );
    write(
        root,
        "collections/billing/invoices/create.req.toml",
        &format!(
            "id = \"{CREATE}\"\nname = \"Create invoice\"\nmethod = \"POST\"\n\
             url = \"{{{{base_url}}}}/v1/invoices\"\n"
        ),
    );
    write(
        root,
        "environments/staging.env.toml",
        &format!(
            "id = \"{STAGING}\"\nname = \"staging\"\n\n\
             [[variables]]\nkey = \"host\"\nvalue = \"staging.test\"\n\n\
             [[variables]]\nkey = \"api_key\"\nsecret = true\n"
        ),
    );
    dir
}

/// The window over [`export_workspace`] with `host` answering its dialogs.
pub(crate) struct Fixture<'a> {
    _dir: TempDir,
    _state: TempDir,
    pub(crate) out: TempDir,
    pub(crate) shell: Entity<Shell>,
    pub(crate) host: Rc<TestExportHost>,
    pub(crate) cx: &'a mut VisualTestContext,
}

pub(crate) fn fixture(cx: &mut TestAppContext) -> Fixture<'_> {
    let dir = export_workspace();
    let (state, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let out = match TempDir::new() {
        Ok(out) => out,
        Err(failure) => panic!("temp dir: {failure}"),
    };
    let host = Rc::new(TestExportHost::default());
    let installed: Rc<dyn ExportHost> = host.clone();
    shell.update(cx, |shell, _| shell.export.host = installed);
    Fixture {
        _dir: dir,
        _state: state,
        out,
        shell,
        host,
        cx,
    }
}

impl Fixture<'_> {
    /// The save dialog answers `name` in the output directory; returns that path.
    pub(crate) fn save_to(&self, name: &str) -> PathBuf {
        let path = self.out.path().join(name);
        *self.host.path.borrow_mut() = Some(path.clone());
        path
    }

    /// Opens the row menu of `id` and picks Export….
    pub(crate) fn export_row(&mut self, id: &str) {
        let row = present(self.cx, &format!("tree-row-{id}"));
        self.cx
            .simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
        self.cx
            .simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::none());
        self.cx.run_until_parked();
        click(self.cx, "tree-menu-export");
    }

    pub(crate) fn engine<R>(&mut self, read: impl FnOnce(&Engine) -> R) -> R {
        let shell = self.shell.clone();
        self.cx
            .update(|_, cx| match &shell.read(cx).workspace {
                OpenedWorkspace::Loaded(engine) => Some(read(engine)),
                OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
            })
            .unwrap_or_else(|| panic!("no workspace"))
    }

    /// Puts `value` in the secret store as the collection secret `key`.
    pub(crate) fn store_secret(&mut self, key: &str, value: &str) {
        self.engine(|engine| {
            let key = SecretKey {
                workspace_id: engine.workspace().manifest.id,
                scope: SecretScope::Collection(COLLECTION.parse().unwrap_or_else(|_| panic!("id"))),
                key: key.to_owned(),
            };
            if let Err(failure) = engine.secret_store().set(&key, value) {
                panic!("secret store: {failure}");
            }
        });
    }

    pub(crate) fn label(&mut self, id: &str) -> Option<String> {
        tree_label(self.cx, id)
    }
}

pub(crate) fn read(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(failure) => panic!("read {}: {failure}", path.display()),
    }
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    match fs::metadata(path) {
        Ok(metadata) => metadata.permissions().mode() & 0o777,
        Err(failure) => panic!("metadata {}: {failure}", path.display()),
    }
}

/// The Postman document the engine makes of `selector`, as its file holds it.
fn postman_text(fixture: &mut Fixture<'_>, selector: &str, include: bool) -> String {
    fixture.engine(|engine| match engine.export_postman(selector, include) {
        Ok(export) => export.text().unwrap_or_else(|failure| panic!("{failure}")),
        Err(failure) => panic!("export: {failure}"),
    })
}

#[gpui::test]
fn the_tree_menu_opens_the_dialog_with_the_row_as_its_scope(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export Billing API · collection")
    );
    present(f.cx, "export-format-postman");
    present(f.cx, "export-format-bundle");
    present(f.cx, "export-format-docs");
    click(f.cx, "export-cancel");
    assert!(bounds(f.cx, "export-dialog").is_none());

    f.export_row(FOLDER);
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export invoices · folder")
    );
    assert!(
        bounds(f.cx, "export-format-docs").is_none(),
        "API docs cover a whole collection"
    );
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    assert!(bounds(f.cx, "export-dialog").is_none());
}

#[gpui::test]
fn postman_without_secrets_writes_the_engines_document(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.save_to("billing.json");
    f.export_row(COLLECTION);
    assert_eq!(
        f.label("export-secrets-note").as_deref(),
        Some("Off: 2 secrets are exported with empty values.")
    );
    assert_eq!(f.label("export-confirm").as_deref(), Some("Save as…"));
    click(f.cx, "export-confirm");

    assert!(bounds(f.cx, "export-dialog").is_none());
    assert_eq!(
        f.host.suggested.borrow().as_slice(),
        ["billing-api.postman_collection.json".to_owned()]
    );
    let expected = postman_text(&mut f, COLLECTION, false);
    assert_eq!(read(&path), expected);
    assert_eq!(
        pushed(f.cx),
        vec![success(
            "Exported to Postman",
            &format!("Billing API · 1 request\n{}", path.display())
        )]
    );
}

#[gpui::test]
fn postman_with_secrets_warns_and_writes_an_owner_only_file(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.store_secret("access_token", "tok-123");
    let path = f.save_to("billing.json");
    f.export_row(COLLECTION);
    assert!(bounds(f.cx, "export-plaintext-warning").is_none());
    click(f.cx, "export-include-secrets");
    assert_eq!(
        f.label("export-plaintext-warning").as_deref(),
        Some(
            "Secrets are written in plain text. Anyone who can read the file can read \
             access_token and client_secret. The file is saved readable only by you \
             (mode 0600); Postman keeps them unencrypted too."
        )
    );
    assert_eq!(
        f.label("export-confirm").as_deref(),
        Some("Save with secrets…")
    );
    click(f.cx, "export-confirm");

    let expected = postman_text(&mut f, COLLECTION, true);
    assert!(expected.contains("tok-123"));
    assert_eq!(read(&path), expected);
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o600);
    assert_eq!(
        pushed(f.cx),
        vec![warning(
            "Exported with empty secrets",
            &format!(
                "Billing API · 1 secret had no value\n{}\nclient_secret is empty in this \
                 workspace and was written empty",
                path.display()
            )
        )]
    );
}

#[gpui::test]
fn show_in_folder_reveals_the_exported_file(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.save_to("invoices.json");
    f.export_row(FOLDER);
    click(f.cx, "export-confirm");
    assert_eq!(
        tree_label(f.cx, "toast-action-1-0").as_deref(),
        Some("Show in folder")
    );
    click_node(f.cx, "toast-action-1-0");
    assert_eq!(f.host.revealed.borrow().as_slice(), [path]);
}

#[gpui::test]
fn a_dismissed_save_dialog_writes_nothing(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    f.export_row(COLLECTION);
    click(f.cx, "export-confirm");
    assert_eq!(f.host.suggested.borrow().len(), 1);
    assert!(pushed(f.cx).is_empty());
    assert!(fs::read_dir(f.out.path()).map_or(0, Iterator::count) == 0);
}

#[gpui::test]
fn a_write_that_fails_shows_why_and_try_again_reopens_the_dialog(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.out.path().join("missing").join("billing.json");
    *f.host.path.borrow_mut() = Some(path.clone());
    f.export_row(COLLECTION);
    click(f.cx, "export-confirm");
    let toasts = pushed(f.cx);
    let Some(toast) = toasts.first() else {
        panic!("no toast");
    };
    assert_eq!(toast.kind, error("", "").kind);
    assert_eq!(toast.title, "Export failed");
    assert!(
        toast
            .text
            .starts_with(&format!("could not write {}: ", path.display())),
        "{}",
        toast.text
    );
    assert_eq!(
        tree_label(f.cx, "toast-action-1-0").as_deref(),
        Some("Try again…")
    );
    click_node(f.cx, "toast-action-1-0");
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export Billing API · collection")
    );
}

#[gpui::test]
fn a_file_chosen_in_the_save_dialog_replaces_the_one_there(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.save_to("billing.json");
    if let Err(failure) = fs::write(&path, "keep") {
        panic!("write: {failure}");
    }
    f.export_row(COLLECTION);
    click(f.cx, "export-confirm");
    let expected = postman_text(&mut f, COLLECTION, false);
    assert_eq!(
        read(&path),
        expected,
        "the OS dialog already asked to replace it"
    );
    assert_eq!(
        pushed(f.cx).first().map(|toast| toast.title.clone()),
        Some("Exported to Postman".to_owned())
    );
}

#[gpui::test]
fn an_environment_exports_from_its_menu_as_a_postman_environment(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let path = f.save_to("staging.json");
    click(f.cx, "rail-environments");
    click(f.cx, &format!("variables-scope-environment-{STAGING}"));
    click(f.cx, &format!("variables-scope-menu-{STAGING}"));
    click(f.cx, "variables-menu-export");
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export staging · environment")
    );
    assert!(bounds(f.cx, "export-format-bundle").is_none());
    assert!(bounds(f.cx, "export-format-docs").is_none());
    assert_eq!(
        f.label("export-secrets-note").as_deref(),
        Some("Off: api_key is exported with an empty value.")
    );
    click(f.cx, "export-confirm");

    assert_eq!(
        f.host.suggested.borrow().as_slice(),
        ["staging.postman_environment.json".to_owned()]
    );
    let expected = postman_text(&mut f, STAGING, false);
    assert_eq!(read(&path), expected);
    assert_eq!(
        pushed(f.cx),
        vec![success(
            "Environment exported",
            &format!("staging · 2 variables\n{}", path.display())
        )]
    );
}

#[gpui::test]
fn the_workspace_menu_exports_the_whole_workspace_as_a_bundle(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    click(f.cx, "workspace-selector");
    click(f.cx, "workspace-export");
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export Demo · whole workspace")
    );
    present(f.cx, "export-format-bundle");
    assert!(bounds(f.cx, "export-format-postman").is_none());
    assert!(bounds(f.cx, "export-format-docs").is_none());
}

#[gpui::test]
fn the_palette_exports_the_selected_row_else_the_active_request(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    click(f.cx, &format!("tree-row-{FOLDER}"));
    run(&mut f, "export");
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export invoices · folder")
    );
    click(f.cx, "export-cancel");

    run(&mut f, "export-workspace");
    assert_eq!(
        f.label("export-dialog").as_deref(),
        Some("Export Demo · whole workspace")
    );
}

/// Runs the command `name` as the palette does.
fn run(f: &mut Fixture<'_>, name: &str) {
    let Some(id) = super::super::commands::COMMANDS
        .iter()
        .find(|command| command.name == name)
        .map(|command| command.id)
    else {
        panic!("no command {name}");
    };
    let shell = f.shell.clone();
    f.cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.run_command(id, window, cx));
    });
    f.cx.run_until_parked();
}
