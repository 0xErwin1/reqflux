//! Documentation (feedback-nov3 S11): the Docs sub-tab of a request, first in the row, and the
//! description block of a collection's settings section.

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::Shell;
use crate::shell::draft_tests::{bounds, click, marker, present, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000080";
const DOCUMENTED: &str = "01J00000000000000000000081";
const PLAIN: &str = "01J00000000000000000000082";
const KEPT: &str = "01J00000000000000000000083";
const COLLECTION_FILE: &str = "collections/api/collection.toml";
const DOCUMENTED_FILE: &str = "collections/api/documented.req.toml";
const PLAIN_FILE: &str = "collections/api/plain.req.toml";

fn request(id: &str, name: &str, extra: &str) -> String {
    format!(
        "id = \"{id}\"\nname = \"{name}\"\n{extra}method = \"GET\"\nurl = \"https://h/{name}\"\n"
    )
}

/// A workspace with a documented request, a plain one, one whose description an earlier
/// Postman import kept in `extensions.postman`, and an undocumented collection.
fn docs_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        COLLECTION_FILE,
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        DOCUMENTED_FILE,
        &request(
            DOCUMENTED,
            "documented",
            "description = \"# Documented\\n\\nSays hello.\"\n",
        ),
    );
    write(root, PLAIN_FILE, &request(PLAIN, "plain", ""));
    let kept = request(KEPT, "kept", "")
        + "\n[extensions]\npostman = '{\"description\":\"Kept by an import\"}'\n";
    write(root, "collections/api/kept.req.toml", &kept);
    dir
}

fn read(root: &std::path::Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_default()
}

fn open_docs(cx: &mut VisualTestContext, id: &str) {
    click(cx, &format!("tree-row-{id}"));
    click(cx, "request-tab-docs");
}

/// Types `text` into the focused editor, one keystroke per character.
fn type_text(cx: &mut VisualTestContext, text: &str) {
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

fn draft_description(
    cx: &mut TestAppContext,
    shell: &Entity<Shell>,
    id: &str,
) -> Option<Option<String>> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(id)
            .map(|draft| draft.request.description.clone())
    })
}

#[gpui::test]
fn docs_is_the_first_sub_tab(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{PLAIN}"));

    let docs = present(cx, "request-tab-docs");
    let params = present(cx, "request-tab-params");
    assert!(docs.origin.x < params.origin.x, "Docs sits left of Params");
}

#[gpui::test]
fn a_documented_request_shows_its_markdown_and_an_edit_button(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_docs(cx, DOCUMENTED);

    present(cx, "request-docs-view");
    present(cx, "request-docs-view-body");
    present(cx, "request-docs-edit");
    assert!(bounds(cx, "request-docs-empty").is_none());
    assert!(bounds(cx, "request-docs-input").is_none());
    assert_eq!(draft_description(cx, &shell, DOCUMENTED), None);
}

#[gpui::test]
fn a_request_without_documentation_offers_to_add_it(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);

    open_docs(cx, PLAIN);

    present(cx, "request-docs-empty");
    present(cx, "request-docs-add");
    assert!(bounds(cx, "request-docs-view").is_none());
    assert!(bounds(cx, "request-docs-edit").is_none());
}

#[gpui::test]
fn a_description_kept_by_an_earlier_import_shows_without_rewriting_the_file(
    cx: &mut TestAppContext,
) {
    let dir = docs_workspace();
    let before = read(dir.path(), "collections/api/kept.req.toml");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);

    open_docs(cx, KEPT);

    present(cx, "request-docs-view");
    assert!(bounds(cx, "request-docs-empty").is_none());
    assert_eq!(draft_description(cx, &shell, KEPT), None);
    assert_eq!(read(dir.path(), "collections/api/kept.req.toml"), before);
}

#[gpui::test]
fn typing_in_the_editor_lands_in_the_draft_and_ctrl_s_saves_the_description(
    cx: &mut TestAppContext,
) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_docs(cx, PLAIN);

    click(cx, "request-docs-add");
    present(cx, "request-docs-input");
    assert_eq!(
        draft_description(cx, &shell, PLAIN),
        None,
        "opening the editor edits nothing"
    );
    type_text(cx, "Hi");
    cx.simulate_keystrokes("enter");
    type_text(cx, "there");

    assert_eq!(
        draft_description(cx, &shell, PLAIN),
        Some(Some("Hi\nthere".to_owned()))
    );
    assert!(marker(cx, PLAIN), "the edit marks the tab");
    assert!(
        !read(dir.path(), PLAIN_FILE).contains("description"),
        "nothing is written before Ctrl+S"
    );

    click(cx, "request-docs-done");
    assert!(
        bounds(cx, "request-docs-input").is_none(),
        "Done closes the editor"
    );
    present(cx, "request-docs-view");
    assert!(marker(cx, PLAIN), "Done does not save");

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let file = read(dir.path(), PLAIN_FILE);
    assert!(
        file.contains("description = \"\"\"\nHi\nthere\"\"\"\n"),
        "the request file holds the description: {file}"
    );
    assert!(!marker(cx, PLAIN), "the save clears the mark");
}

#[gpui::test]
fn clearing_the_text_clears_a_kept_description_too(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_docs(cx, KEPT);

    click(cx, "request-docs-edit");
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.run_until_parked();

    assert_eq!(draft_description(cx, &shell, KEPT), Some(None));
    click(cx, "request-docs-done");
    present(cx, "request-docs-empty");
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    let file = read(dir.path(), "collections/api/kept.req.toml");
    assert!(!file.contains("Kept by an import"), "{file}");
    assert!(!file.contains("description"), "{file}");
}

fn open_collection(cx: &mut VisualTestContext) {
    click(cx, "rail-environments");
    click(cx, &format!("variables-scope-collection-{COLLECTION}"));
}

#[gpui::test]
fn the_collection_section_adds_documentation_through_the_engine(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_collection(cx);

    present(cx, "collection-docs-empty");
    click(cx, "collection-docs-add");
    present(cx, "collection-docs-input");
    type_text(cx, "AboutTheAPI");
    assert!(
        !read(dir.path(), COLLECTION_FILE).contains("description"),
        "typing writes nothing"
    );

    click(cx, "collection-docs-done");

    let file = read(dir.path(), COLLECTION_FILE);
    assert!(file.contains("description = \"AboutTheAPI\""), "{file}");
    assert!(bounds(cx, "collection-docs-input").is_none());
    present(cx, "collection-docs-view");
    present(cx, "collection-docs-view-body");
    present(cx, "collection-docs-edit");
}

#[gpui::test]
fn cancelling_the_collection_editor_writes_nothing(cx: &mut TestAppContext) {
    let dir = docs_workspace();
    let before = read(dir.path(), COLLECTION_FILE);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_collection(cx);

    click(cx, "collection-docs-add");
    type_text(cx, "Never saved");
    click(cx, "collection-docs-cancel");

    assert_eq!(read(dir.path(), COLLECTION_FILE), before);
    present(cx, "collection-docs-empty");
    assert!(bounds(cx, "collection-docs-input").is_none());
}
