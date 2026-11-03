//! The variable popover (T12, S10, design/variable-popover/01-inline-popover.html): a
//! `{{name}}` mark opens it after a 400 ms hover or on a click, and it edits the value of the
//! winning scope in place, or adds an undefined variable.

use std::time::Duration;

use gpui::{Entity, Modifiers, Point, TestAppContext, VisualTestContext, px};
use reqflux_engine::{SecretKey, SecretScope};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J000000000000000000000V1";
const REQUEST: &str = "01J000000000000000000000V2";
const STAGING: &str = "01J000000000000000000000V3";
const WORKSPACE: &str = "01J00000000000000000000900";

/// A workspace whose collection `API` declares `base`, whose environment Staging declares
/// `base` and the secret `token`, and whose request `GET Vars` goes to
/// `{{base}}/ok/{{token}}/{{tenant}}` with the header `X-Base: {{base}}`; `tenant` is
/// defined nowhere.
fn popover_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"API\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"http://h\"\n"
        ),
    );
    write(
        root,
        "environments/staging.env.toml",
        &format!(
            "id = \"{STAGING}\"\nname = \"Staging\"\n\n\
             [[variables]]\nkey = \"base\"\nvalue = \"https://staging\"\n\n\
             [[variables]]\nkey = \"token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/api/vars.req.toml",
        &format!(
            "id = \"{REQUEST}\"\nname = \"Vars\"\nmethod = \"GET\"\n\
             url = \"{{{{base}}}}/ok/{{{{token}}}}/{{{{tenant}}}}\"\n\n\
             [[headers]]\nkey = \"X-Base\"\nvalue = \"{{{{base}}}}\"\n"
        ),
    );
    dir
}

/// Opens the request and, with `environment`, makes it active.
fn open(cx: &mut VisualTestContext, shell: &Entity<Shell>, environment: Option<&str>) {
    click(cx, &format!("tree-row-{REQUEST}"));
    if let Some(slug) = environment {
        shell.update(cx, |shell, cx| {
            shell.set_environment(Some(slug.to_owned()), cx);
        });
        cx.run_until_parked();
    }
}

fn popover_open(cx: &mut VisualTestContext) -> bool {
    bounds(cx, "variable-popover").is_some()
}

fn url_editing(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|_, cx| shell.read(cx).url_edit.is_some())
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    let typed = text
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&typed);
    cx.run_until_parked();
}

fn popover_value(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .variable_popover
            .as_ref()
            .map(|popover| popover.input.read(cx).value().to_string())
            .unwrap_or_default()
    })
}

fn current(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    scope: SecretScope,
    key: &str,
) -> Option<String> {
    cx.update(|_, cx| {
        let shell = shell.read(cx);
        let stored = shell
            .state
            .as_ref()
            .and_then(|state| state.current_values(WORKSPACE).ok())
            .and_then(|values| values.get(scope, key).map(str::to_owned));
        assert_eq!(
            stored.as_deref(),
            shell.current.get(scope, key),
            "the app state and the window disagree"
        );
        stored
    })
}

fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(REQUEST)
            .map(|state| state.inspection.unresolved.clone())
            .unwrap_or_default()
    })
}

fn collection() -> SecretScope {
    SecretScope::Collection(id(COLLECTION))
}

fn staging() -> SecretScope {
    SecretScope::Environment(id(STAGING))
}

fn id(text: &str) -> reqflux_core::model::Id {
    match text.parse() {
        Ok(id) => id,
        Err(error) => panic!("id {text}: {error:?}"),
    }
}

fn secret(cx: &mut VisualTestContext, shell: &Entity<Shell>, key: &str) -> Option<String> {
    cx.update(|_, cx| match &shell.read(cx).workspace {
        OpenedWorkspace::Loaded(engine) => {
            let key = SecretKey {
                workspace_id: id(WORKSPACE),
                scope: staging(),
                key: key.to_owned(),
            };
            match engine.secret_store().get(&key) {
                Ok(value) => value,
                Err(error) => panic!("secret store: {error}"),
            }
        }
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => panic!("no workspace"),
    })
}

#[gpui::test]
fn hovering_a_mark_for_400_ms_opens_the_popover(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    let mark = present(cx, "request-url-var-1");
    cx.simulate_mouse_move(mark.center(), None, Modifiers::none());
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    assert!(!popover_open(cx), "the popover opened before 400 ms");

    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert!(popover_open(cx), "the popover did not open after 400 ms");
    assert!(!url_editing(cx, &shell));
}

#[gpui::test]
fn clicking_a_mark_opens_the_popover_instead_of_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    click(cx, "request-url-var-1");

    assert!(popover_open(cx));
    assert!(!url_editing(cx, &shell), "the click started the URL edit");
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn clicking_the_url_outside_a_mark_still_edits_it(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    let field = present(cx, "request-url");
    let outside = Point::new(field.right() - px(12.), field.center().y);
    cx.simulate_click(outside, Modifiers::none());
    cx.run_until_parked();

    assert!(url_editing(cx, &shell));
    assert!(!popover_open(cx));
}

#[gpui::test]
fn enter_saves_the_current_value_of_the_winning_scope(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    let before = std::fs::read(dir.path().join("environments/staging.env.toml")).ok();

    click(cx, "request-url-var-1");
    assert_eq!(popover_value(cx, &shell), "https://staging");
    cx.simulate_keystrokes("ctrl-a");
    type_text(cx, "http://x");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    assert_eq!(
        current(cx, &shell, staging(), "base").as_deref(),
        Some("http://x")
    );
    assert_eq!(current(cx, &shell, collection(), "base"), None);
    let after = std::fs::read(dir.path().join("environments/staging.env.toml")).ok();
    assert_eq!(before, after, "the popover wrote the file");
    assert!(!unresolved(cx, &shell).contains(&"base".to_owned()));
}

#[gpui::test]
fn a_secret_stays_masked_and_saves_to_the_keyring(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    assert!(unresolved(cx, &shell).contains(&"token".to_owned()));

    click(cx, "request-url-var-2");
    assert!(popover_open(cx));
    assert!(bounds(cx, "variable-popover-reveal").is_some());
    type_text(cx, "s3cret");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(secret(cx, &shell, "token").as_deref(), Some("s3cret"));
    assert_eq!(current(cx, &shell, staging(), "token"), None);
    assert!(!unresolved(cx, &shell).contains(&"token".to_owned()));

    // Opened again, the stored value is not loaded until revealed.
    click(cx, "request-url-var-2");
    assert_eq!(popover_value(cx, &shell), "");
    click(cx, "variable-popover-reveal");
    assert_eq!(popover_value(cx, &shell), "s3cret");
}

#[gpui::test]
fn quick_add_creates_the_variable_in_the_chosen_scope(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));
    assert!(unresolved(cx, &shell).contains(&"tenant".to_owned()));

    click(cx, "request-url-var-3");
    assert!(bounds(cx, "variable-popover-not-defined").is_some());
    // Targets: Staging, API, Globals.
    click(cx, "variable-popover-target-3");
    click(cx, "variable-popover-input");
    type_text(cx, "tnt_7f2a");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    let manifest = std::fs::read_to_string(dir.path().join("reqflux.toml")).unwrap_or_default();
    assert!(manifest.contains("key = \"tenant\""), "{manifest}");
    assert!(manifest.contains("value = \"tnt_7f2a\""), "{manifest}");
    assert!(!unresolved(cx, &shell).contains(&"tenant".to_owned()));
}

#[gpui::test]
fn escape_closes_the_popover_without_writing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);

    click(cx, "request-url-var-1");
    cx.simulate_keystrokes("ctrl-a");
    type_text(cx, "zzz");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(!popover_open(cx));
    assert_eq!(current(cx, &shell, collection(), "base"), None);
}

#[gpui::test]
fn f2_on_a_reference_opens_the_popover_while_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (shell_dir, state_path) = state_file();
    let _keep = shell_dir;
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));

    cx.simulate_keystrokes("home right right right f2");
    cx.run_until_parked();

    assert!(popover_open(cx));
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn alt_click_on_a_reference_opens_the_popover_while_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    let mark = present(cx, "request-url-var-1");
    let field = present(cx, "request-url");
    cx.simulate_click(
        Point::new(field.right() - px(12.), field.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(url_editing(cx, &shell));

    cx.simulate_click(mark.center(), Modifiers::alt());
    cx.run_until_parked();

    assert!(popover_open(cx));
    assert!(url_editing(cx, &shell), "the URL edit closed");
}

#[gpui::test]
fn a_mark_in_a_grid_cell_opens_the_popover_instead_of_editing(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, None);
    click(cx, "request-tab-headers");

    click(cx, "request-headers-value-1-var-1");

    assert!(popover_open(cx));
    assert!(
        cx.update(|_, cx| shell.read(cx).cell_edit.is_none()),
        "the click started the cell edit"
    );
    assert_eq!(popover_value(cx, &shell), "http://h");
}

#[gpui::test]
fn typing_in_the_opened_popover_replaces_the_value(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-1");
    type_text(cx, "http://x");

    assert_eq!(popover_value(cx, &shell), "http://x");
}

#[gpui::test]
fn the_mark_keeps_its_own_label_while_its_popover_is_open(cx: &mut TestAppContext) {
    let dir = popover_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open(cx, &shell, Some("staging"));

    click(cx, "request-url-var-1");
    assert!(popover_open(cx));

    assert_eq!(
        label_of(cx, "request-url-var-1").as_deref(),
        Some("{{base}}")
    );
}

/// The accessible label of element `id`, if it is shown.
fn label_of(cx: &mut VisualTestContext, id: &str) -> Option<String> {
    bounds(cx, id)?;
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}
