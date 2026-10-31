//! The Scripts pane edits the draft's scripts: the editors, the empty state's button, the
//! delete button, their persistence and the save into the sidecars.

use std::path::Path;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{Request, Scripts};
use reqflux_engine::state::{AppState, Draft};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, render, state_file};
use super::drafts::{ScriptDraft, decode_payload, encode_payload};
use super::request::{SubTab, sub_tab_badge};
use super::scripts_edit::ScriptSlot;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000071";
const SCRIPTED: &str = "01J00000000000000000000072";
const FILE: &str = "collections/api/scripted.req.toml";
const PRE: &str = "collections/api/scripted.pre.js";
const TEST: &str = "collections/api/scripted.test.js";

/// A workspace holding `GET Scripted` with a pre-request sidecar and no test script.
fn scripts_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        FILE,
        &format!(
            "id = \"{SCRIPTED}\"\nname = \"Scripted\"\nmethod = \"GET\"\nurl = \"https://h/s\"\n"
        ),
    );
    write(root, PRE, "pre();\n");
    dir
}

/// Opens `GET Scripted` on its Scripts sub-tab.
fn open_scripts(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{SCRIPTED}"));
    click(cx, "request-tab-scripts");
}

fn draft_scripts(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<Scripts> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(SCRIPTED)
            .and_then(|draft| draft.scripts.as_ref())
            .map(|draft| draft.scripts.clone())
    })
}

fn has_draft(cx: &mut TestAppContext, shell: &Entity<Shell>) -> bool {
    cx.update(|cx| shell.read(cx).drafts.contains_key(SCRIPTED))
}

/// What the empty state's `Write a test` button does. The response pane may cover the
/// button while the split is short, so the test calls it rather than clicking.
fn write_test(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.start_script_edit(SCRIPTED, ScriptSlot::Test, window, cx);
        });
    });
    cx.run_until_parked();
}

/// Types `text` into the focused editor, one keystroke per character.
fn type_text(cx: &mut VisualTestContext, text: &str) {
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

fn read(root: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(root.join(relative)).ok()
}

fn scripts(pre: Option<&str>, test: Option<&str>) -> Scripts {
    Scripts {
        pre_request: pre.map(str::to_owned),
        test: test.map(str::to_owned),
    }
}

#[gpui::test]
fn the_pane_stacks_both_editors_with_the_empty_state_for_a_missing_script(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);

    for id in [
        "request-script-note",
        "request-script-pre-editor",
        "request-script-pre-header",
        "request-script-pre-line-1",
        "request-script-pre-delete",
        "request-script-test-editor",
        "request-script-test-empty",
        "request-script-test-create",
    ] {
        assert!(bounds(cx, id).is_some(), "{id} is shown");
    }
    assert!(bounds(cx, "request-script-test-delete").is_none());
    assert!(bounds(cx, "request-script-pre-empty").is_none());
    let pre = bounds(cx, "request-script-pre-editor").map(|b| b.origin.y);
    let test = bounds(cx, "request-script-test-editor").map(|b| b.origin.y);
    assert!(pre < test, "the pre-request editor sits above the test one");
    assert!(!has_draft(cx, &shell), "showing the pane edits nothing");
}

#[gpui::test]
fn writing_a_test_lands_in_the_draft_survives_a_restart_and_saves_its_sidecar(
    cx: &mut TestAppContext,
) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);

    write_test(cx, &shell);
    assert!(
        bounds(cx, "request-script-test-input").is_some(),
        "the editor opened"
    );
    assert!(!has_draft(cx, &shell), "opening the editor edits nothing");
    type_text(cx, "ok();");
    assert_eq!(
        draft_scripts(cx, &shell),
        Some(scripts(Some("pre();\n"), Some("ok();")))
    );
    assert!(marker(cx, SCRIPTED), "the edit marks the tab");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "request-script-test-input").is_none());
    assert!(
        bounds(cx, "request-script-test-line-1").is_some(),
        "the script shows"
    );

    let (shell_2, cx) = render(cx, &dir, &state_path);
    assert_eq!(
        draft_scripts(cx, &shell_2),
        Some(scripts(Some("pre();\n"), Some("ok();"))),
        "the draft's scripts were restored"
    );

    open_scripts(cx);
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert_eq!(read(dir.path(), TEST).as_deref(), Some("ok();"));
    assert_eq!(read(dir.path(), PRE).as_deref(), Some("pre();\n"));
    assert!(!has_draft(cx, &shell_2), "the save discards the draft");
    assert!(!marker(cx, SCRIPTED));
    let file = read(dir.path(), FILE).unwrap_or_default();
    assert!(
        !file.contains("ok()"),
        "the request file holds no script: {file}"
    );
}

#[gpui::test]
fn emptying_or_deleting_a_script_deletes_its_sidecar_on_save(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);

    click(cx, "request-script-pre-line-1");
    let seeded = cx.update(|_, cx| {
        shell
            .read(cx)
            .script_edit
            .as_ref()
            .map(|edit| edit.input.read(cx).value().to_string())
    });
    assert_eq!(
        seeded.as_deref(),
        Some("pre();\n"),
        "seeded with the script"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.run_until_parked();
    assert_eq!(draft_scripts(cx, &shell), Some(scripts(Some(""), None)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        bounds(cx, "request-script-pre-empty").is_some(),
        "an empty script shows the empty state"
    );
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        read(dir.path(), PRE).is_none(),
        "the empty script's sidecar is gone"
    );

    write(dir.path(), PRE, "again();\n");
    let (shell_2, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);
    click(cx, "request-script-pre-delete");
    assert_eq!(draft_scripts(cx, &shell_2), Some(scripts(None, None)));
    assert!(bounds(cx, "request-script-pre-empty").is_some());
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        read(dir.path(), PRE).is_none(),
        "the deleted script's sidecar is gone"
    );
}

#[gpui::test]
fn a_sidecar_changed_behind_the_draft_makes_save_conflict(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);

    write_test(cx, &shell);
    type_text(cx, "mine();");
    cx.simulate_keystrokes("escape");
    write(dir.path(), TEST, "theirs();\n");
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    let conflicted = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(SCRIPTED)
            .is_some_and(|draft| draft.conflicted)
    });
    assert!(conflicted, "the draft is kept, conflicted");
    assert_eq!(read(dir.path(), TEST).as_deref(), Some("theirs();\n"));
    assert_eq!(read(dir.path(), PRE).as_deref(), Some("pre();\n"));
}

#[gpui::test]
fn a_request_file_changed_behind_the_draft_leaves_the_sidecars_untouched(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);

    write_test(cx, &shell);
    type_text(cx, "mine();");
    cx.simulate_keystrokes("escape");
    shell.update(cx, |shell, cx| {
        shell.set_draft_url(SCRIPTED, "https://h/mine".to_owned(), cx);
    });
    let theirs = format!(
        "id = \"{SCRIPTED}\"\nname = \"Scripted\"\nmethod = \"GET\"\nurl = \"https://h/theirs\"\n"
    );
    write(dir.path(), FILE, &theirs);
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    let conflicted = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(SCRIPTED)
            .is_some_and(|draft| draft.conflicted)
    });
    assert!(conflicted, "the draft is kept, conflicted");
    assert!(
        read(dir.path(), TEST).is_none(),
        "no test sidecar was written"
    );
    assert_eq!(read(dir.path(), PRE).as_deref(), Some("pre();\n"));
    assert_eq!(read(dir.path(), FILE), Some(theirs));
}

#[gpui::test]
fn the_scripts_and_settings_tabs_badge_what_the_shown_request_has(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_scripts(cx);
    let badge = |cx: &mut VisualTestContext, tab: SubTab| {
        cx.update(|_, cx| {
            let shell = shell.read(cx);
            shell
                .requests
                .get(SCRIPTED)
                .and_then(|state| sub_tab_badge(SCRIPTED, state, shell, tab))
        })
    };

    assert_eq!(badge(cx, SubTab::Scripts).as_deref(), Some("pre"));
    assert!(bounds(cx, "request-tab-scripts-count").is_some());
    assert_eq!(badge(cx, SubTab::Settings), None);
    assert!(bounds(cx, "request-tab-settings-count").is_none());

    shell.update(cx, |shell, cx| {
        shell.edit_draft_scripts(
            SCRIPTED,
            |scripts| scripts.test = Some("t();".to_owned()),
            cx,
        );
        shell.edit_draft(
            SCRIPTED,
            |request| {
                request.settings.timeout_ms = Some(500);
                request.settings.verify_tls = Some(false);
            },
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(badge(cx, SubTab::Scripts).as_deref(), Some("2"));
    assert_eq!(badge(cx, SubTab::Settings).as_deref(), Some("2"));
    assert!(bounds(cx, "request-tab-settings-count").is_some());

    shell.update(cx, |shell, cx| {
        shell.edit_draft_scripts(
            SCRIPTED,
            |scripts| scripts.pre_request = Some(String::new()),
            cx,
        );
    });
    assert_eq!(badge(cx, SubTab::Scripts).as_deref(), Some("test"));
    shell.update(cx, |shell, cx| {
        shell.edit_draft_scripts(SCRIPTED, |scripts| scripts.test = None, cx);
    });
    cx.run_until_parked();
    assert_eq!(badge(cx, SubTab::Scripts), None);
    assert!(bounds(cx, "request-tab-scripts-count").is_none());
}

#[gpui::test]
fn a_draft_stored_before_scripts_were_editable_still_loads(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let mut request = match toml::from_str::<Request>(&read(dir.path(), FILE).unwrap_or_default()) {
        Ok(request) => request,
        Err(error) => panic!("request: {error}"),
    };
    request.url = "https://h/old-draft".to_owned();
    let payload = match toml::to_string(&request) {
        Ok(payload) => payload,
        Err(error) => panic!("payload: {error}"),
    };
    let workspace_id = {
        let manifest = read(dir.path(), "reqflux.toml").unwrap_or_default();
        let table = manifest.parse::<toml::Table>().unwrap_or_default();
        table
            .get("id")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match AppState::open(&state_path) {
        Ok(mut state) => {
            if let Err(error) = state.put_draft(&workspace_id, SCRIPTED, Draft::new(payload, None))
            {
                panic!("put draft: {error}");
            }
        }
        Err(error) => panic!("open state: {error}"),
    }

    let (shell, cx) = render(cx, &dir, &state_path);
    let url = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(SCRIPTED)
            .map(|draft| (draft.request.url.clone(), draft.scripts.is_none()))
    });
    assert_eq!(url, Some(("https://h/old-draft".to_owned(), true)));
}

#[gpui::test]
fn a_draft_without_script_edits_saves_the_request_and_leaves_the_sidecars(cx: &mut TestAppContext) {
    let dir = scripts_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{SCRIPTED}"));
    shell.update(cx, |shell, cx| {
        shell.set_draft_url(SCRIPTED, "https://h/changed".to_owned(), cx);
    });
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    let file = read(dir.path(), FILE).unwrap_or_default();
    assert!(
        file.contains("https://h/changed"),
        "the request was saved: {file}"
    );
    assert_eq!(read(dir.path(), PRE).as_deref(), Some("pre();\n"));
    assert!(read(dir.path(), TEST).is_none());
}

#[test]
fn the_payload_round_trips_the_request_and_its_scripts() {
    let request = match toml::from_str::<Request>(
        "id = \"01J00000000000000000000072\"\nname = \"S\"\nmethod = \"GET\"\nurl = \"u\"\n",
    ) {
        Ok(request) => request,
        Err(error) => panic!("request: {error}"),
    };
    let edited = ScriptDraft {
        scripts: scripts(Some("a();\n"), None),
        pre_request_base: Some("hash-a".to_owned()),
        test_base: None,
    };
    for scripts in [None, Some(edited)] {
        let payload = match encode_payload(&request, scripts.as_ref()) {
            Ok(payload) => payload,
            Err(error) => panic!("encode: {error}"),
        };
        match decode_payload(&payload) {
            Ok(decoded) => assert_eq!(decoded, (request.clone(), scripts)),
            Err(error) => panic!("decode: {error}"),
        }
    }
}
