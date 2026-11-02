//! `Import Postman collection…` from the workspace menu (app-pending S6): pick a file,
//! review what the importer would write, import it into the open workspace; errors in the
//! dialog; Cancel and Escape write nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{Entity, TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::super::Shell;
use super::super::toasts::tests as toasts;
use super::super::workspace_picker::TestPicker;
use super::super::workspace_switch_tests::{bounds, click, pair, present, render, temp_dir};
use super::{NOTHING_WRITTEN, TestImportPicker, size_label};

const BASIC: &str = include_str!("../../../reqflux_postman/tests/fixtures/collection-basic.json");

/// The collection the fixture writes, under the workspace root.
const COLLECTION: &str = "collections/basic-collection/collection.toml";

/// `text` written to `name` in a fresh directory, kept alive by the returned guard.
fn document(name: &str, text: &str) -> (TempDir, PathBuf) {
    let dir = temp_dir();
    let path = dir.path().join(name);
    if let Err(error) = fs::write(&path, text) {
        panic!("write {}: {error}", path.display());
    }
    (dir, path)
}

fn automation(cx: &mut VisualTestContext) -> gpui_mcp::Automation {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    automation
}

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

/// Every label in the automation tree.
fn labels(automation: &gpui_mcp::Automation) -> Vec<String> {
    automation
        .snapshot()
        .nodes
        .values()
        .filter_map(|node| node.label.clone())
        .collect()
}

/// Opens the dialog from the workspace menu, the file dialog answering `file`.
fn open(cx: &mut VisualTestContext, shell: &Entity<Shell>, file: Option<PathBuf>) {
    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker { file });
    });
    click(cx, "workspace-selector");
    click(cx, "workspace-import");
}

/// The files of the workspace at `root`, to prove a step wrote nothing.
fn written(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => panic!("read {}: {error}", dir.display()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

#[gpui::test]
fn the_dialog_reviews_the_file_then_imports_it_into_the_open_workspace(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("basic.postman_collection.json", BASIC);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    let before = written(pair.first.path());
    open(cx, &shell, Some(file.clone()));

    present(cx, "import-backdrop");
    assert!(bounds(cx, "workspace-menu").is_none(), "the menu closes");
    assert_eq!(
        label(&automation, "import-file").as_deref(),
        Some(file.display().to_string().as_str())
    );
    assert_eq!(
        label(&automation, "import-detected").as_deref(),
        Some(format!("Postman collection · {}", size_label(BASIC.len())).as_str())
    );
    assert_eq!(
        label(&automation, "import-into").as_deref(),
        Some("into First")
    );
    for (kind, text) in [
        ("collection", "Collection 1"),
        ("folders", "Folders 1"),
        ("requests", "Requests 3"),
        ("scripts", "Scripts 2"),
    ] {
        assert_eq!(
            label(&automation, &format!("import-count-{kind}")).as_deref(),
            Some(text)
        );
    }
    assert_eq!(
        label(&automation, "import-file-1").as_deref(),
        Some(COLLECTION),
        "an open workspace keeps its manifest"
    );
    let files = labels(&automation)
        .iter()
        .filter(|text| text.starts_with("collections/"))
        .count();
    assert_eq!(
        label(&automation, "import-files-count"),
        Some(format!("{files} files"))
    );
    assert_eq!(
        label(&automation, "import-note").as_deref(),
        Some(NOTHING_WRITTEN)
    );
    assert!(bounds(cx, "import-error").is_none());
    assert_eq!(
        written(pair.first.path()),
        before,
        "a review writes nothing"
    );

    click(cx, "import-confirm");
    assert!(bounds(cx, "import-backdrop").is_none());
    assert!(pair.first.path().join(COLLECTION).is_file());
    assert!(
        labels(&automation)
            .iter()
            .any(|text| text.contains("Basic Collection")),
        "the sidebar shows the imported collection"
    );
}

#[gpui::test]
fn a_file_that_is_not_a_postman_collection_shows_why_and_imports_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("hello.json", r#"{ "hello": "world" }"#);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    let before = written(pair.first.path());
    open(cx, &shell, Some(file));

    let error = label(&automation, "import-error").unwrap_or_default();
    assert!(
        error.starts_with("This is not a Postman collection."),
        "{error}"
    );
    assert!(bounds(cx, "import-count-requests").is_none());
    click(cx, "import-confirm");
    present(cx, "import-backdrop");
    assert_eq!(written(pair.first.path()), before);
}

#[gpui::test]
fn a_file_that_is_not_json_says_so(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("broken.json", "{ not json");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    open(cx, &shell, Some(file));

    let error = label(&automation, "import-error").unwrap_or_default();
    assert!(error.starts_with("The file is not JSON."), "{error}");
}

#[gpui::test]
fn a_collection_already_in_the_workspace_is_refused_before_import(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("basic.json", BASIC);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    open(cx, &shell, Some(file.clone()));
    click(cx, "import-confirm");
    let before = written(pair.first.path());

    open(cx, &shell, Some(file));
    let error = label(&automation, "import-error").unwrap_or_default();
    assert!(
        error.starts_with("The workspace already has what this file holds."),
        "{error}"
    );
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(written(pair.first.path()), before);
}

#[gpui::test]
fn cancel_and_escape_close_the_dialog_and_write_nothing(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("basic.json", BASIC);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let before = written(pair.first.path());
    open(cx, &shell, Some(file.clone()));
    present(cx, "import-count-requests");
    click(cx, "import-cancel");
    assert!(bounds(cx, "import-backdrop").is_none());

    open(cx, &shell, Some(file));
    present(cx, "import-count-requests");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "import-backdrop").is_none());
    assert_eq!(written(pair.first.path()), before);
}

#[gpui::test]
fn a_dismissed_file_dialog_leaves_the_dialog_waiting_for_a_file(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    let automation = automation(cx);
    open(cx, &shell, None);
    present(cx, "import-browse");
    assert_eq!(
        label(&automation, "import-file").as_deref(),
        Some("Choose a Postman collection (.json)")
    );
    assert!(bounds(cx, "import-error").is_none());

    let (_file_dir, file) = document("basic.json", BASIC);
    shell.update(cx, |shell, _| {
        shell.import.picker = Rc::new(TestImportPicker { file: Some(file) });
    });
    click(cx, "import-browse");
    present(cx, "import-count-requests");
}

#[test]
fn sizes_read_as_the_artboard_writes_them() {
    assert_eq!(size_label(512), "512 B");
    assert_eq!(size_label(86 * 1024), "86 KB");
    assert_eq!(size_label(86 * 1024 + 1), "87 KB");
    assert_eq!(size_label(3 * 1024 * 1024 / 2), "1.5 MB");
}

#[gpui::test]
fn an_import_shows_its_counts_and_the_workspace_in_a_toast(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("basic.postman_collection.json", BASIC);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    open(cx, &shell, Some(file));
    assert!(toasts::shown_toasts(cx).is_empty());

    click(cx, "import-confirm");
    assert_eq!(
        toasts::pushed(cx),
        vec![toasts::success(
            "Imported 3 requests into First",
            "Basic Collection · 1 folder"
        )]
    );
    assert_eq!(toasts::shown_toasts(cx), vec!["toast-1".to_owned()]);
}

#[gpui::test]
fn an_import_that_fails_shows_why_in_a_toast(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_file_dir, file) = document("basic.postman_collection.json", BASIC);
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    open(cx, &shell, Some(file));
    // Something writes the collection after the review: the import refuses to overwrite it.
    let taken = pair.first.path().join(COLLECTION);
    if let Some(parent) = taken.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        panic!("create {}: {error}", parent.display());
    }
    if let Err(error) = fs::write(&taken, "") {
        panic!("write {}: {error}", taken.display());
    }

    click(cx, "import-confirm");
    present(cx, "import-error");
    let toasts = toasts::pushed(cx);
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    let Some(toast) = toasts.first() else {
        panic!("no toast");
    };
    assert_eq!(toast.title, "Import failed");
    assert!(toast.text.contains("already exists"), "{}", toast.text);
    assert_eq!(toasts::shown_toasts(cx), vec!["toast-1".to_owned()]);
}
