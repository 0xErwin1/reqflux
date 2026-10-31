//! Editing variables on the workspace settings page (variables-editor S6, S7, and S2 on the
//! window side): every commit writes at once, current values override the shared value,
//! secrets move to the secret store and a refused write shows its reason.

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::state::AppState;
use reqflux_engine::{MASK, SecretKey, SecretScope};

use super::Shell;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::{Server, closed_port, serve};
use super::variables_tests::{USE, enter, read, send_use, type_text, variables_workspace};
use crate::workspace::OpenedWorkspace;

const MANIFEST: &str = "reqflux.toml";

fn open_globals(cx: &mut VisualTestContext) {
    click(cx, "rail-environments");
    click(cx, "variables-scope-globals");
}

/// Opens the cell `selector`, replaces its text with `text` and commits.
fn commit(cx: &mut VisualTestContext, selector: &str, text: &str) {
    click(cx, selector);
    if text.is_empty() {
        cx.simulate_keystrokes("ctrl-a backspace");
        cx.run_until_parked();
    } else {
        type_text(cx, text);
    }
    enter(cx);
}

/// The table and row of the open cell field.
fn editing(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<(String, usize)> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .cell_edit
            .as_ref()
            .map(|edit| (edit.table.clone(), edit.n))
    })
}

fn unresolved(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<usize> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .requests
            .get(USE)
            .map(|state| state.inspection.unresolved.len())
    })
}

fn stored_secret(cx: &mut VisualTestContext, shell: &Entity<Shell>, key: &str) -> Option<String> {
    cx.update(|_, cx| match &shell.read(cx).workspace {
        OpenedWorkspace::Loaded(engine) => {
            let key = SecretKey {
                workspace_id: engine.workspace().manifest.id,
                scope: SecretScope::Global,
                key: key.to_owned(),
            };
            match engine.secret_store().get(&key) {
                Ok(value) => value,
                Err(error) => panic!("secret store: {error}"),
            }
        }
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => None,
    })
}

#[gpui::test]
fn adding_a_variable_writes_the_file_and_opens_its_key(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);

    click(cx, "variables-add");
    assert!(read(dir.path(), MANIFEST).contains("key = \"new_variable\""));
    assert_eq!(
        editing(cx, &shell),
        Some(("variables-key".to_owned(), 2)),
        "the new key opens"
    );
    type_text(cx, "fresh");
    enter(cx);
    let manifest = read(dir.path(), MANIFEST);
    assert!(manifest.contains("key = \"fresh\""));
    assert!(!manifest.contains("new_variable"));
    present(cx, "variables-row-2");

    click(cx, "variables-add");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    click(cx, "variables-add");
    let manifest = read(dir.path(), MANIFEST);
    assert!(manifest.contains("key = \"new_variable\""));
    assert!(manifest.contains("key = \"new_variable_2\""), "{manifest}");
}

#[gpui::test]
fn adding_a_referenced_variable_reinspects_open_requests(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    click(cx, "variables-delete-1");
    assert!(!read(dir.path(), MANIFEST).contains("greeting"));
    click(cx, "rail-collections");
    click(cx, &format!("tree-row-{USE}"));
    assert_eq!(unresolved(cx, &shell), Some(1));

    open_globals(cx);
    click(cx, "variables-add");
    type_text(cx, "greeting");
    enter(cx);
    assert_eq!(unresolved(cx, &shell), Some(0));
}

#[gpui::test]
fn editing_a_shared_value_writes_the_file_and_the_next_send(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = variables_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    commit(cx, "variables-value-1", "bye");
    assert!(read(dir.path(), MANIFEST).contains("value = \"bye\""));
    commit(cx, "variables-description-1", "said at the door");
    assert!(read(dir.path(), MANIFEST).contains("description = \"said at the door\""));

    click(cx, "variables-enabled-1");
    assert!(read(dir.path(), MANIFEST).contains("enabled = false"));
    click(cx, "variables-enabled-1");
    assert!(!read(dir.path(), MANIFEST).contains("enabled = false"));

    assert_eq!(
        send_use(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/bye"))
    );
}

#[gpui::test]
fn a_current_value_overrides_the_shared_one_until_cleared(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = variables_workspace(port);
    let (_state_dir, state_path) = state_file();
    let before = read(dir.path(), MANIFEST);
    {
        let (shell, cx) = render(cx, &dir, &state_path);
        open_globals(cx);
        commit(cx, "variables-current-1", "mine");
        assert_eq!(read(dir.path(), MANIFEST), before, "never in the file");
        assert_eq!(
            send_use(cx, &shell),
            Some(format!("http://127.0.0.1:{port}/use/mine"))
        );
    }

    let (shell, cx) = render(cx, &dir, &state_path);
    assert_eq!(
        send_use(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/mine")),
        "the current value survives a new shell"
    );
    open_globals(cx);
    commit(cx, "variables-current-1", "");
    assert_eq!(
        send_use(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/hello"))
    );
    let stored = match AppState::open(&state_path) {
        Ok(state) => state.current_values("01J00000000000000000000900"),
        Err(error) => panic!("open state: {error}"),
    };
    match stored {
        Ok(values) => assert_eq!(values.get(SecretScope::Global, "greeting"), None),
        Err(error) => panic!("current values: {error}"),
    }
}

#[gpui::test]
fn marking_a_variable_secret_moves_its_value_to_the_secret_store(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = variables_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    assert!(bounds(cx, "variables-masked-1").is_none());

    click(cx, "variables-secret-1");
    assert!(!read(dir.path(), MANIFEST).contains("hello"));
    assert!(read(dir.path(), MANIFEST).contains("secret = true"));
    assert_eq!(
        stored_secret(cx, &shell, "greeting").as_deref(),
        Some("hello")
    );
    present(cx, "variables-masked-1");

    // The initial value is not in the file and does not open; the current value field opens
    // empty; an empty commit keeps the value, a new one writes the store only.
    click(cx, "variables-value-1");
    assert_eq!(editing(cx, &shell), None);
    commit(cx, "variables-current-1", "");
    assert_eq!(
        stored_secret(cx, &shell, "greeting").as_deref(),
        Some("hello")
    );
    commit(cx, "variables-current-1", "hidden");
    assert_eq!(
        stored_secret(cx, &shell, "greeting").as_deref(),
        Some("hidden")
    );
    assert!(!read(dir.path(), MANIFEST).contains("hidden"));

    // The send resolves the secret: history records it redacted.
    assert_eq!(
        send_use(cx, &shell),
        Some(format!("http://127.0.0.1:{port}/use/{MASK}"))
    );
    assert_eq!(unresolved(cx, &shell), Some(0));

    open_globals(cx);
    assert!(bounds(cx, "variables-warning").is_none());
    click(cx, "variables-secret-1");
    present(cx, "variables-warning");
    assert!(!read(dir.path(), MANIFEST).contains("secret = true"));
    assert!(!read(dir.path(), MANIFEST).contains("hidden"));
    assert_eq!(stored_secret(cx, &shell, "greeting"), None);
}

#[gpui::test]
fn a_duplicate_key_shows_the_error_and_leaves_the_file(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    click(cx, "variables-add");
    enter(cx);
    let before = read(dir.path(), MANIFEST);
    assert!(bounds(cx, "variables-error").is_none());

    commit(cx, "variables-key-2", "greeting");
    present(cx, "variables-error");
    assert_eq!(read(dir.path(), MANIFEST), before);
    present(cx, "variables-row-2");

    // The next successful write clears it.
    commit(cx, "variables-key-2", "other");
    assert!(bounds(cx, "variables-error").is_none());
}

#[gpui::test]
fn an_undeclared_current_value_shows_and_can_be_cleared(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    {
        let mut state = match AppState::open(&state_path) {
            Ok(state) => state,
            Err(error) => panic!("open state: {error}"),
        };
        if let Err(error) = state.set_current_value(
            "01J00000000000000000000900",
            SecretScope::Global,
            "script_token",
            "from-script",
        ) {
            panic!("set current value: {error}");
        }
    }
    let (shell, cx) = render(cx, &dir, &state_path);
    open_globals(cx);
    present(cx, "variables-undeclared-1");

    commit(cx, "variables-undeclared-current-1", "");
    assert!(bounds(cx, "variables-undeclared-1").is_none());
    let left = cx.update(|_, cx| {
        shell
            .read(cx)
            .current
            .get(SecretScope::Global, "script_token")
            .map(str::to_owned)
    });
    assert_eq!(left, None);
}
