//! The failure toasts of app-pending L11: a write the app state refuses while the user acts
//! (removing a recent workspace, picking the active environment, a send's script writes), an
//! OS dialog that fails (a dismissed one shows nothing), a history entry that cannot be
//! resent, and a reload that fails after a save or an import. The app state refuses writes
//! here because the test reopens it read-only under the shell.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::state::AppState;

use super::Shell;
use super::current_value_tests;
use super::draft_tests::{self, PLAIN, edit_url, state_file};
use super::history_tests;
use super::import_flow::TestImportPicker;
use super::send_tests::{Server, closed_port, serve, settle};
use super::toasts::tests::{error, pushed, shown_toasts};
use super::toasts::{Kind, PushedToast};
use super::workspace_picker::TestPicker;
use super::workspace_switch_tests::{click, pair, render};
use crate::workspace_tests::write;

/// Swaps the shell's app state for a read-only connection to the same file.
fn refuse_state_writes(cx: &mut VisualTestContext, shell: &Entity<Shell>, path: &Path) {
    let state = match AppState::open_read_only(path) {
        Ok(state) => state,
        Err(error) => panic!("open read-only: {error}"),
    };
    shell.update(cx, |shell, _| shell.state = Some(state));
}

/// The one toast pushed, which must be an error titled `title`; its details are returned.
fn one_error(cx: &mut VisualTestContext, title: &str) -> String {
    let toasts = pushed(cx);
    let [
        PushedToast {
            kind,
            title: shown,
            text,
        },
    ] = toasts.as_slice()
    else {
        panic!("expected one toast: {toasts:?}");
    };
    assert_eq!((*kind, shown.as_str()), (Kind::Error, title), "{toasts:?}");
    assert_eq!(shown_toasts(cx), vec!["toast-1".to_owned()]);
    text.clone()
}

#[gpui::test]
fn removing_a_recent_the_state_refuses_says_so(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    refuse_state_writes(cx, &shell, &pair.state_path);
    click(cx, "workspace-selector");
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.remove_recent(1, cx)));
    cx.run_until_parked();

    let details = one_error(cx, "Cannot remove Second from the recent workspaces");
    assert!(details.contains("state.sqlite3"), "{details}");
}

#[gpui::test]
fn an_active_environment_the_state_refuses_says_so(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    refuse_state_writes(cx, &shell, &pair.state_path);
    click(cx, "environment-selector");
    click(cx, "environment-option-dev");

    let details = one_error(cx, "Cannot remember the active environment");
    assert!(details.contains("state.sqlite3"), "{details}");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).active_environment.clone())
            .as_deref(),
        Some("dev"),
        "the environment is still active for this session"
    );
}

#[gpui::test]
fn script_writes_the_state_refuses_say_so_without_their_values(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = current_value_tests::workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = draft_tests::render(cx, &dir, &state_path);
    current_value_tests::select_dev(cx, &shell);
    refuse_state_writes(cx, &shell, &state_path);
    draft_tests::click(cx, &format!("tree-row-{}", current_value_tests::LOGIN));
    draft_tests::click(cx, "request-send");
    assert_eq!(
        settle(cx, &shell, current_value_tests::LOGIN).as_deref(),
        Some("200 OK")
    );

    let details = one_error(cx, "Cannot remember the values the scripts set");
    assert!(!details.contains("abc"), "no value in a toast: {details}");
}

#[gpui::test]
fn a_folder_dialog_that_fails_says_so_and_a_dismissed_one_shows_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "workspace-selector");
    click(cx, "workspace-open");
    click(cx, "workspace-selector");
    click(cx, "workspace-new-folder");
    assert!(pushed(cx).is_empty(), "a dismissed dialog shows nothing");

    shell.update(cx, |shell, _| {
        shell.picker = Rc::new(TestPicker {
            failure: Some("no portal".to_owned()),
            ..TestPicker::default()
        });
    });
    click(cx, "workspace-selector");
    click(cx, "workspace-open");
    click(cx, "workspace-selector");
    click(cx, "workspace-new-folder");
    assert_eq!(
        pushed(cx),
        vec![
            error("Cannot open the folder dialog", "no portal"),
            error("Cannot open the new workspace dialog", "no portal"),
        ]
    );
}

#[gpui::test]
fn a_file_dialog_that_fails_says_so(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker::default());
    });
    click(cx, "workspace-selector");
    click(cx, "workspace-import");
    assert!(pushed(cx).is_empty(), "a dismissed dialog shows nothing");

    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker {
            failure: Some("no portal".to_owned()),
            ..TestImportPicker::default()
        });
    });
    click(cx, "import-browse");
    assert_eq!(
        pushed(cx),
        vec![error("Cannot open the file dialog", "no portal")]
    );
}

#[gpui::test]
fn a_recorded_request_that_does_not_parse_cannot_be_resent(cx: &mut TestAppContext) {
    let dir = history_tests::history_workspace(serve(Server::Ok));
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = draft_tests::render(cx, &dir, &state_path);
    draft_tests::click(cx, &format!("tree-row-{}", history_tests::PING));
    draft_tests::click(cx, "request-send");
    assert_eq!(
        settle(cx, &shell, history_tests::PING).as_deref(),
        Some("200 OK")
    );
    history_tests::open_top_row(cx);
    shell.update(cx, |shell, _| {
        let key = shell.orphan.as_ref().map(|tab| tab.key());
        let reopened = key
            .and_then(|key| shell.sends.get_mut(&key))
            .and_then(|send| send.history.as_mut());
        match reopened {
            Some(reopened) => reopened.document = "token = \"s3cret\" [".to_owned(),
            None => panic!("no reopened entry"),
        }
    });

    draft_tests::click(cx, "history-resend");
    assert_eq!(
        pushed(cx),
        vec![error(
            "Cannot resend this entry",
            "The recorded request could not be read."
        )]
    );
}

/// Breaks the manifest of the workspace at `root`, so the next reload fails.
fn break_manifest(root: &Path) {
    write(root, "reqflux.toml", "id = [");
}

#[gpui::test]
fn a_save_whose_reload_fails_says_so(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_tests::draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = draft_tests::render(cx, &dir, &state_path);
    draft_tests::click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, &format!("http://127.0.0.1:{port}/edited"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    break_manifest(dir.path());
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    assert_eq!(
        pushed(cx),
        vec![error(
            "Saved, but the workspace could not be reloaded",
            RELOAD_DETAILS
        )]
    );
}

const RELOAD_DETAILS: &str = "Reopen the workspace to see what is wrong with its files.";

#[gpui::test]
fn an_import_whose_reload_fails_says_so(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let file_dir = super::workspace_switch_tests::temp_dir();
    let file: PathBuf = file_dir.path().join("basic.postman_collection.json");
    write(
        file_dir.path(),
        "basic.postman_collection.json",
        include_str!("../../../reqflux_postman/tests/fixtures/collection-basic.json"),
    );
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker {
            file: Some(file),
            ..TestImportPicker::default()
        });
    });
    click(cx, "workspace-selector");
    click(cx, "workspace-import");
    break_manifest(pair.first.path());
    click(cx, "import-confirm");

    let toasts = pushed(cx);
    assert_eq!(
        toasts.last(),
        Some(&error(
            "Imported, but the workspace could not be reloaded",
            RELOAD_DETAILS
        )),
        "{toasts:?}"
    );
}
