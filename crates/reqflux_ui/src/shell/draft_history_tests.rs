//! Draft undo and redo (feedback-nov3 S4): the history rules, the strip buttons and the
//! keys, and the data a mode switch keeps.

use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{Auth, Body, RawLanguage, Scripts};

use super::{BURST, DEPTH, DraftHistories, Slot};
use crate::shell::Shell;
use crate::shell::auth_edit_tests as auth;
use crate::shell::body_edit_tests as body;
use crate::shell::draft_tests::{
    bounds, click, draft_workspace, edit_url, marker, render, state_file,
};
use crate::shell::drafts::{RequestDraft, ScriptDraft};
use crate::shell::send_tests::closed_port;
use crate::workspace_tests::write;

const PLAIN: &str = crate::shell::draft_tests::PLAIN;
const SCRIPT_PRE: &str = "collections/api/plain.pre.js";

fn draft_url(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(PLAIN)
            .map(|draft| draft.request.url.clone())
    })
}

fn draft_with_url(url: &str) -> Option<RequestDraft> {
    let mut draft = RequestDraft::test_default();
    draft.request.url = url.to_owned();
    Some(draft)
}

#[test]
fn a_typing_burst_in_one_slot_is_one_undo_step() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    for step in 0..5u32 {
        histories.record(
            "a",
            draft_with_url(&step.to_string()),
            Slot::Url,
            start + Duration::from_millis(100 * u64::from(step)),
        );
    }
    let restored = histories.step("a", true, draft_with_url("5"));
    assert_eq!(
        restored,
        Some(draft_with_url("0")),
        "back to before the burst"
    );
    assert_eq!(
        histories.step("a", true, draft_with_url("0")),
        None,
        "one step only"
    );
}

#[test]
fn a_pause_or_another_slot_starts_a_new_step() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    histories.record("a", draft_with_url("0"), Slot::Url, start);
    histories.record("a", draft_with_url("1"), Slot::Url, start + BURST * 2);
    histories.record("a", draft_with_url("2"), Slot::Method, start + BURST * 2);
    assert!(histories.can_undo("a"));
    let mut seen = Vec::new();
    let mut current = draft_with_url("3");
    while let Some(previous) = histories.step("a", true, current.clone()) {
        seen.push(previous.clone());
        current = previous;
    }
    assert_eq!(
        seen,
        [
            draft_with_url("2"),
            draft_with_url("1"),
            draft_with_url("0")
        ]
    );
}

#[test]
fn a_structural_edit_never_joins_a_burst() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    histories.record("a", draft_with_url("0"), Slot::Structural, start);
    histories.record("a", draft_with_url("1"), Slot::Structural, start);
    assert!(histories.step("a", true, draft_with_url("2")).is_some());
    assert!(histories.step("a", true, draft_with_url("1")).is_some());
}

#[test]
fn the_history_keeps_the_last_depth_steps() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    for step in 0..DEPTH + 10 {
        histories.record(
            "a",
            draft_with_url(&step.to_string()),
            Slot::Structural,
            start,
        );
    }
    let mut steps = 0;
    let mut current = draft_with_url("end");
    while let Some(previous) = histories.step("a", true, current) {
        steps += 1;
        current = previous;
    }
    assert_eq!(steps, DEPTH);
}

#[test]
fn a_new_edit_drops_the_redo_steps() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    histories.record("a", draft_with_url("0"), Slot::Structural, start);
    let undone = histories.step("a", true, draft_with_url("1"));
    assert_eq!(undone, Some(draft_with_url("0")));
    assert!(histories.can_redo("a"));
    histories.record("a", draft_with_url("0"), Slot::Structural, start);
    assert!(!histories.can_redo("a"));
}

#[test]
fn history_is_per_request() {
    let mut histories = DraftHistories::default();
    histories.record("a", draft_with_url("0"), Slot::Structural, Instant::now());
    assert!(histories.can_undo("a"));
    assert!(!histories.can_undo("b"));
    histories.forget("a");
    assert!(!histories.can_undo("a"));
}

#[test]
fn a_save_moves_every_step_onto_the_saved_file() {
    let mut histories = DraftHistories::default();
    let start = Instant::now();
    let saved = RequestDraft::test_default().request;
    let mut edited = RequestDraft::test_default();
    edited.request.url = "1".to_owned();
    edited.base_hash = Some("old".to_owned());
    edited.conflicted = true;
    edited.scripts = Some(ScriptDraft {
        scripts: reqflux_core::model::Scripts::default(),
        pre_request_base: Some("old-script".to_owned()),
        test_base: None,
    });
    // One step back to "no draft" and one to the edited draft, above it.
    histories.record("a", None, Slot::Structural, start);
    histories.record("a", Some(edited), Slot::Structural, start);

    histories.settle_after_save(
        "a",
        saved.clone(),
        None,
        Some("new".to_owned()),
        Some((Some("new-script".to_owned()), None)),
    );

    let Some(step) = histories.step("a", true, None) else {
        panic!("the edited step survived the save");
    };
    let Some(step) = step else {
        panic!("the edited step is a draft");
    };
    assert_eq!(step.base_hash.as_deref(), Some("new"), "rebased");
    assert!(!step.conflicted, "a save resolves the conflict");
    let scripts = step.scripts.expect("the edited scripts stayed");
    assert_eq!(scripts.pre_request_base.as_deref(), Some("new-script"));
    assert_eq!(scripts.test_base, None);

    let Some(step) = histories.step("a", true, None) else {
        panic!("the step that was no draft survived the save");
    };
    let Some(step) = step else {
        panic!("the file now holds the save, so that step carries the request it had");
    };
    assert_eq!(step.request, saved);
    assert_eq!(step.base_hash.as_deref(), Some("new"));
    assert!(!step.conflicted);
    assert!(step.scripts.is_none(), "the scripts were as saved");
}

#[test]
fn a_save_without_a_hash_to_rebase_onto_drops_the_history() {
    let mut histories = DraftHistories::default();
    histories.record("a", None, Slot::Structural, Instant::now());
    histories.settle_after_save("a", RequestDraft::test_default().request, None, None, None);
    assert!(!histories.can_undo("a"));
}

#[test]
fn a_save_that_wrote_scripts_carries_the_old_text_in_its_no_draft_step() {
    let mut histories = DraftHistories::default();
    let saved = RequestDraft::test_default().request;
    histories.record("a", None, Slot::Structural, Instant::now());
    histories.settle_after_save(
        "a",
        saved.clone(),
        Some(Scripts {
            pre_request: Some("old();".to_owned()),
            test: None,
        }),
        Some("new".to_owned()),
        Some((Some("new-script".to_owned()), None)),
    );
    let step = histories
        .step("a", true, None)
        .flatten()
        .expect("the step that was no draft survived the save");
    let scripts = step
        .scripts
        .expect("the step carries the scripts the sidecars held");
    assert_eq!(scripts.scripts.pre_request.as_deref(), Some("old();"));
    assert_eq!(scripts.scripts.test, None);
    assert_eq!(scripts.pre_request_base.as_deref(), Some("new-script"));
    assert_eq!(scripts.test_base, None);

    // A save that wrote no scripts keeps the step without them, as before.
    histories.record("b", None, Slot::Structural, Instant::now());
    histories.settle_after_save(
        "b",
        saved,
        None,
        Some("new".to_owned()),
        Some((Some("new-script".to_owned()), None)),
    );
    let step = histories
        .step("b", true, None)
        .flatten()
        .expect("the step that was no draft survived the save");
    assert!(step.scripts.is_none(), "the scripts were as saved");
}

#[gpui::test]
fn the_undo_and_redo_buttons_walk_the_draft(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    click(cx, "request-action-undo");
    assert!(draft_url(cx, &shell).is_none(), "nothing to undo yet");

    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    assert!(marker(cx, PLAIN));
    click(cx, "request-action-undo");
    assert!(!marker(cx, PLAIN), "the whole typing burst is one step");
    assert!(draft_url(cx, &shell).is_none(), "back to the saved request");

    click(cx, "request-action-redo");
    assert_eq!(draft_url(cx, &shell).as_deref(), Some(edited.as_str()));
    assert!(marker(cx, PLAIN));
}

#[gpui::test]
fn the_keys_undo_and_redo_when_no_editor_has_the_focus(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert!(draft_url(cx, &shell).is_none(), "Ctrl+Z undid the draft");
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert_eq!(draft_url(cx, &shell).as_deref(), Some(edited.as_str()));
    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert!(draft_url(cx, &shell).is_none());
    cx.simulate_keystrokes("ctrl-y");
    cx.run_until_parked();
    assert_eq!(
        draft_url(cx, &shell).as_deref(),
        Some(edited.as_str()),
        "Ctrl+Y redoes"
    );
}

#[gpui::test]
fn an_editor_keeps_its_own_ctrl_z(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, &format!("http://127.0.0.1:{port}/edited"));

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    let can_undo = cx.update(|_, cx| cx.global::<DraftHistories>().can_undo(PLAIN));
    assert!(
        can_undo,
        "the draft history was not consumed by the field's undo"
    );
    assert!(draft_url(cx, &shell).is_some());
}

#[gpui::test]
fn discarding_the_changes_can_be_undone(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);

    click(cx, "request-action-revert");
    assert!(draft_url(cx, &shell).is_none(), "discarded");
    click(cx, "request-action-undo");
    assert_eq!(
        draft_url(cx, &shell).as_deref(),
        Some(edited.as_str()),
        "one undo restores it"
    );
    assert!(marker(cx, PLAIN));
}

#[gpui::test]
fn a_save_keeps_the_history_and_undo_restores_the_draft(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let file = dir.path().join("collections/api/plain.req.toml");
    click(cx, &format!("tree-row-{PLAIN}"));
    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(draft_url(cx, &shell).is_none(), "saving drops the draft");
    let saved = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(saved.contains(&edited), "the save wrote the edit: {saved}");

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert_eq!(
        draft_url(cx, &shell).as_deref(),
        Some("{{base}}/ok"),
        "Ctrl+Z after a save takes the edit the save committed back as a draft"
    );
}

#[gpui::test]
fn a_draft_undone_after_a_save_saves_on_top_of_the_saved_file(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let file = dir.path().join("collections/api/plain.req.toml");
    click(cx, &format!("tree-row-{PLAIN}"));
    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let restored = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(
        restored.contains("{{base}}/ok"),
        "the restored draft saved over the file the save wrote: {restored}"
    );
    assert!(
        !restored.contains("/edited"),
        "the edit is gone from the file: {restored}"
    );
    assert!(
        draft_url(cx, &shell).is_none(),
        "the second save dropped it"
    );
}

#[gpui::test]
fn undoing_a_save_that_wrote_scripts_takes_the_old_text_back(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    write(dir.path(), SCRIPT_PRE, "old();\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    shell.update(cx, |shell, cx| {
        shell.edit_draft_scripts(
            PLAIN,
            |scripts| scripts.pre_request = Some("edited();\n".to_owned()),
            cx,
        );
    });
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let written = std::fs::read_to_string(dir.path().join(SCRIPT_PRE)).unwrap_or_default();
    assert!(
        written.contains("edited();"),
        "the save wrote the edit: {written}"
    );

    click(cx, "request-action-undo");
    cx.run_until_parked();
    let restored = cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(PLAIN)
            .and_then(|draft| draft.scripts.as_ref())
            .map(|scripts| scripts.scripts.pre_request.clone())
    });
    assert_eq!(
        restored,
        Some(Some("old();\n".to_owned())),
        "one undo takes the script the save replaced back as a draft"
    );
}

#[gpui::test]
fn redoing_an_undone_save_goes_back_to_the_saved_file(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    let edited = format!("http://127.0.0.1:{port}/edited");
    edit_url(cx, &edited);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    cx.simulate_keystrokes("ctrl-z");
    cx.run_until_parked();
    assert!(draft_url(cx, &shell).is_some(), "the draft came back");
    cx.simulate_keystrokes("ctrl-shift-z");
    cx.run_until_parked();
    assert!(
        draft_url(cx, &shell).is_none(),
        "redo drops the draft, so the view shows the file the save wrote"
    );
}

#[gpui::test]
fn switching_the_body_mode_back_restores_the_text(cx: &mut TestAppContext) {
    let dir = body::json_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    body::open_body(cx);

    click(cx, "body-mode-none");
    assert_eq!(body::draft_body(cx, &shell), Some(Body::None));
    click(cx, "body-mode-urlencoded");
    click(cx, "body-mode-raw");
    assert_eq!(
        body::draft_body(cx, &shell),
        Some(body::raw(RawLanguage::Json, "{}")),
        "the raw text came back"
    );
    assert!(
        body::file(&dir).contains("text = \"{}\""),
        "the file is untouched"
    );
}

#[gpui::test]
fn switching_the_auth_type_back_restores_the_fields(cx: &mut TestAppContext) {
    let dir = auth::bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    auth::open_auth(cx);

    auth::pick_type(cx, "basic");
    assert!(matches!(
        auth::draft_auth(cx, &shell),
        Some(Auth::Basic { .. })
    ));
    auth::pick_type(cx, "bearer");
    assert_eq!(
        auth::draft_auth(cx, &shell),
        Some(Auth::Bearer {
            token: "stored-token".to_owned()
        })
    );
}

#[gpui::test]
fn the_strip_buttons_exist_with_a_draft(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = draft_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));
    edit_url(cx, "x");
    for id in ["undo", "redo", "revert", "save"] {
        assert!(
            bounds(cx, &format!("request-action-{id}")).is_some(),
            "{id}"
        );
    }
    let _: &mut VisualTestContext = cx;
}
