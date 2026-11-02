//! The Body pane edits the draft (S4, S5): the mode menu, the raw text editor and the
//! urlencoded and form-data grids, and their persistence (S8).

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{Body, FormPart, RawLanguage};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, render, state_file};
use super::request::answer_body_file_dialog;
use super::send_tests::closed_port;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000081";
const POSTED: &str = "01J00000000000000000000082";
const FILE: &str = "collections/api/posted.req.toml";

/// A workspace holding `POST Posted`, whose body is the `[body]` table `body` and whose
/// only header is `Accept: */*`.
fn body_workspace(body: &str) -> TempDir {
    let port = closed_port();
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
            "id = \"{POSTED}\"\nname = \"Posted\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:{port}/p\"\n\n\
             [[headers]]\nkey = \"Accept\"\nvalue = \"*/*\"\n\n[body]\n{body}"
        ),
    );
    dir
}

fn json_workspace() -> TempDir {
    body_workspace("mode = \"raw\"\nlanguage = \"json\"\ntext = \"{}\"\n")
}

/// Opens `POST Posted` on its Body sub-tab.
fn open_body(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{POSTED}"));
    click(cx, "request-tab-body");
}

fn draft_body(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<Body> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(POSTED)
            .map(|draft| draft.request.body.clone())
    })
}

fn draft_request(
    cx: &mut TestAppContext,
    shell: &Entity<Shell>,
) -> Option<reqflux_core::model::Request> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(POSTED)
            .map(|draft| draft.request.clone())
    })
}

/// Picks the raw language `language` from the language menu of a raw body.
fn pick_language(cx: &mut VisualTestContext, language: &str) {
    click(cx, "request-body-language");
    click(cx, &format!("body-language-{language}"));
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

fn file(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default()
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

/// The message the engine gives for GraphQL `variables` that do not parse.
fn engine_variables_message(text: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(_) => panic!("{text} should not parse as JSON"),
        Err(error) => reqflux_engine::ExecuteError::GraphqlVariables(error.to_string()).to_string(),
    }
}

fn raw(language: RawLanguage, text: &str) -> Body {
    Body::Raw {
        language,
        text: text.to_owned(),
    }
}

#[gpui::test]
fn the_mode_control_switches_the_body_in_the_draft(cx: &mut TestAppContext) {
    let dir = json_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    for mode in ["none", "raw", "urlencoded", "formdata", "binary", "graphql"] {
        assert!(
            bounds(cx, &format!("body-mode-{mode}")).is_some(),
            "every mode shows without opening anything: {mode}"
        );
    }
    click(cx, "body-mode-raw");
    assert!(draft_body(cx, &shell).is_none(), "the same mode is a no-op");

    click(cx, "request-body-language");
    for language in ["json", "xml", "text"] {
        assert!(
            bounds(cx, &format!("body-language-{language}")).is_some(),
            "{language}"
        );
    }
    click(cx, "body-language-json");
    assert!(
        bounds(cx, "body-language-json").is_none(),
        "the pick closed it"
    );
    assert!(
        draft_body(cx, &shell).is_none(),
        "the same language is a no-op"
    );

    pick_language(cx, "xml");
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Xml, "{}")),
        "raw languages keep the text"
    );
    assert!(marker(cx, POSTED), "the edit marks the tab");

    click(cx, "body-mode-urlencoded");
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Urlencoded { fields: Vec::new() })
    );
    assert!(
        bounds(cx, "request-body-language").is_none(),
        "only a raw body has a language"
    );
    click(cx, "body-mode-binary");
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Binary {
            path: String::new()
        })
    );
    click(cx, "body-mode-graphql");
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Graphql {
            query: String::new(),
            variables: String::new()
        })
    );
    click(cx, "body-mode-raw");
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Json, "")),
        "another kind starts empty, in JSON"
    );

    click(cx, "request-body-language");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        bounds(cx, "body-language-text").is_none(),
        "Escape closes it"
    );

    let (shell_2, _cx) = render(cx, &dir, &state_path);
    assert_eq!(
        draft_body(cx, &shell_2),
        Some(raw(RawLanguage::Json, "")),
        "the draft was restored"
    );
}

#[gpui::test]
fn typing_in_the_raw_editor_lands_in_the_draft(cx: &mut TestAppContext) {
    let dir = json_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    click(cx, "request-body-code");
    assert!(
        bounds(cx, "request-body-input").is_some(),
        "the editor opened"
    );
    assert!(draft_body(cx, &shell).is_none(), "opening creates no draft");

    cx.simulate_keystrokes("ctrl-a backspace");
    type_text(cx, "a");
    cx.simulate_keystrokes("enter");
    type_text(cx, "b");
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Json, "a\nb")),
        "Enter inserts a newline and every keystroke lands"
    );
    assert!(marker(cx, POSTED));

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| shell.read(cx).sends.contains_key(POSTED)),
        "Ctrl+Enter sends"
    );
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Json, "a\nb")),
        "Ctrl+Enter inserts no newline"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        bounds(cx, "request-body-input").is_none(),
        "Escape leaves it"
    );
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Json, "a\nb")),
        "Escape keeps the text"
    );

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        file(&dir).contains("text = \"\"\"\na\nb\"\"\"") || file(&dir).contains("a\\nb"),
        "Ctrl+S wrote it: {}",
        file(&dir)
    );
    assert!(draft_body(cx, &shell).is_none(), "saving drops the draft");
}

#[gpui::test]
fn the_raw_editor_shows_its_focus_ring_only_while_it_has_focus(cx: &mut TestAppContext) {
    let dir = json_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    // Focus changes reach the editor only in the active window, as in the app.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    open_body(cx);
    let ring = "request-body-input-focus-ring";
    assert!(bounds(cx, ring).is_none(), "no ring before editing");

    click(cx, "request-body-code");
    assert!(
        bounds(cx, ring).is_some(),
        "the focused editor has the ring"
    );

    click(cx, "request-url");
    assert!(
        bounds(cx, "request-body-input").is_some(),
        "the editor stays open"
    );
    assert!(
        bounds(cx, ring).is_none(),
        "leaving the editor drops the ring"
    );
    assert!(
        draft_body(cx, &shell).is_none(),
        "moving the focus edits nothing"
    );

    click(cx, "request-body-input");
    assert!(bounds(cx, ring).is_some(), "coming back shows it again");
}

#[gpui::test]
fn the_urlencoded_grid_edits_the_draft(cx: &mut TestAppContext) {
    let dir =
        body_workspace("mode = \"urlencoded\"\n\n[[body.fields]]\nkey = \"a\"\nvalue = \"1\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    click(cx, "request-body-value-1");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        draft_body(cx, &shell).is_none(),
        "the shown value is a no-op"
    );

    click(cx, "request-body-enabled-1");
    let Some(Body::Urlencoded { fields }) = draft_body(cx, &shell) else {
        panic!("an urlencoded draft");
    };
    assert_eq!(fields.first().map(|field| field.enabled), Some(false));

    click(cx, "request-body-key-1");
    cx.simulate_keystrokes("ctrl-a backspace");
    type_text(cx, "b");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let Some(Body::Urlencoded { fields }) = draft_body(cx, &shell) else {
        panic!("an urlencoded draft");
    };
    assert_eq!(
        fields
            .first()
            .map(|field| (field.key.as_str(), field.value.as_str())),
        Some(("b", "1"))
    );
    assert!(marker(cx, POSTED));

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(file(&dir).contains("key = \"b\""), "{}", file(&dir));
}

#[gpui::test]
fn a_file_part_value_is_its_path(cx: &mut TestAppContext) {
    let dir = body_workspace(
        "mode = \"formdata\"\n\n[[body.parts]]\nkind = \"file\"\nkey = \"upload\"\n\
         path = \"old.bin\"\n",
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    click(cx, "request-body-value-1");
    cx.simulate_keystrokes("ctrl-a backspace");
    type_text(cx, "new.bin");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let Some(Body::FormData { parts }) = draft_body(cx, &shell) else {
        panic!("a form-data draft");
    };
    assert!(
        matches!(parts.first(), Some(FormPart::File { path, key, .. }) if path == "new.bin" && key == "upload"),
        "{parts:?}"
    );

    let (shell_2, _cx) = render(cx, &dir, &state_path);
    assert!(
        matches!(draft_body(cx, &shell_2), Some(Body::FormData { .. })),
        "the draft was restored"
    );
}

#[gpui::test]
fn a_header_cell_still_writes_the_headers(cx: &mut TestAppContext) {
    let dir =
        body_workspace("mode = \"urlencoded\"\n\n[[body.fields]]\nkey = \"a\"\nvalue = \"1\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));
    click(cx, "request-tab-headers");

    click(cx, "request-headers-key-1");
    cx.simulate_keystrokes("ctrl-a backspace");
    type_text(cx, "X");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let Some(request) = draft_request(cx, &shell) else {
        panic!("a draft");
    };
    assert_eq!(
        request.headers.first().map(|row| row.key.as_str()),
        Some("X")
    );
    assert!(
        matches!(&request.body, Body::Urlencoded { fields } if fields.first().map(|f| f.key.as_str()) == Some("a")),
        "the body is untouched"
    );
}

#[gpui::test]
fn choosing_a_file_fills_the_binary_path_and_clear_empties_it(cx: &mut TestAppContext) {
    let dir = body_workspace("mode = \"binary\"\npath = \"\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    assert_eq!(
        label_of(cx, "request-body-file").as_deref(),
        Some("No file"),
        "the empty well"
    );
    assert!(
        bounds(cx, "request-body-clear").is_none(),
        "nothing to clear"
    );

    let chosen = dir.path().join("contract.pdf");
    cx.update(|_, cx| answer_body_file_dialog(Some(chosen.clone()), cx));
    click(cx, "request-body-choose");
    cx.run_until_parked();
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Binary {
            path: chosen.display().to_string()
        }),
        "the chosen path is the draft's"
    );
    assert_eq!(
        label_of(cx, "request-body-file").as_deref(),
        Some(chosen.display().to_string().as_str()),
        "the well shows the path"
    );
    assert!(
        bounds(cx, "request-body-clear").is_some(),
        "Clear is offered"
    );
    assert!(marker(cx, POSTED), "the edit marks the tab");

    click(cx, "request-body-clear");
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Binary {
            path: String::new()
        })
    );
    assert_eq!(
        label_of(cx, "request-body-file").as_deref(),
        Some("No file"),
        "the well is empty again"
    );
    assert!(
        bounds(cx, "request-body-clear").is_none(),
        "nothing to clear"
    );
}

#[gpui::test]
fn the_graphql_editors_edit_the_draft_and_validate_the_variables(cx: &mut TestAppContext) {
    let dir = body_workspace("mode = \"graphql\"\nquery = \"query Q { ok }\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);
    assert!(draft_body(cx, &shell).is_none(), "no draft before editing");

    click(cx, "request-body-query");
    assert!(
        bounds(cx, "request-body-query-input").is_some(),
        "the query editor opened"
    );
    type_text(cx, "x");
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Graphql {
            query: "xquery Q { ok }".to_owned(),
            variables: String::new()
        }),
        "every keystroke lands in the draft"
    );

    click(cx, "request-body-variables");
    assert!(
        bounds(cx, "request-body-query-input").is_none(),
        "one editor at a time"
    );
    assert!(bounds(cx, "request-body-variables-input").is_some());
    assert_eq!(
        label_of(cx, "request-body-variables-badge").as_deref(),
        Some("VALID JSON"),
        "blank variables are valid, as the engine treats them"
    );

    type_text(cx, "{");
    assert_eq!(
        label_of(cx, "request-body-variables-error").as_deref(),
        Some(engine_variables_message("{").as_str()),
        "the engine's exact message"
    );
    assert_eq!(
        label_of(cx, "request-body-variables-badge").as_deref(),
        Some("NOT VALID JSON")
    );

    cx.simulate_keystrokes("ctrl-a backspace");
    type_text(cx, "{\"id\":\"1\"}");
    assert!(
        bounds(cx, "request-body-variables-error").is_none(),
        "valid JSON shows no banner"
    );
    assert_eq!(
        label_of(cx, "request-body-variables-badge").as_deref(),
        Some("VALID JSON")
    );

    type_text(cx, ",");
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert_eq!(
        draft_body(cx, &shell),
        Some(Body::Graphql {
            query: "xquery Q { ok }".to_owned(),
            variables: "{\"id\":\"1\"},".to_owned()
        }),
        "Ctrl+Enter inserts no newline"
    );
    assert!(
        !cx.update(|_, cx| shell.read(cx).sends.contains_key(POSTED)),
        "Ctrl+Enter refuses the send while the variables fail"
    );

    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| shell.read(cx).sends.contains_key(POSTED)),
        "valid variables send"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        bounds(cx, "request-body-variables-input").is_none(),
        "Escape leaves the editor"
    );
    assert!(
        matches!(draft_body(cx, &shell), Some(Body::Graphql { variables, .. }) if variables == "{\"id\":\"1\"}"),
        "Escape keeps the text"
    );
}

#[gpui::test]
fn switching_the_mode_away_closes_the_graphql_editor(cx: &mut TestAppContext) {
    let dir = body_workspace("mode = \"graphql\"\nquery = \"query Q { ok }\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    click(cx, "request-body-query");
    assert!(bounds(cx, "request-body-query-input").is_some());
    click(cx, "body-mode-raw");
    assert!(
        bounds(cx, "request-body-query-input").is_none(),
        "the editor of the mode that is gone closes"
    );
    assert_eq!(
        draft_body(cx, &shell),
        Some(raw(RawLanguage::Json, "")),
        "the mode switched"
    );
}

#[gpui::test]
fn picking_the_stored_graphql_mode_still_creates_no_draft(cx: &mut TestAppContext) {
    let dir = body_workspace("mode = \"graphql\"\nquery = \"query Q { ok }\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_body(cx);

    click(cx, "body-mode-graphql");
    assert!(draft_body(cx, &shell).is_none(), "the same mode is a no-op");
    assert!(
        bounds(cx, "request-body-query").is_some(),
        "the read-only well"
    );
    assert!(
        bounds(cx, "request-body-query-input").is_none(),
        "nothing opened"
    );
}
