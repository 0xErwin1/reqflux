//! The workspace menu (recent-workspaces S3), switching (S4), the empty state (S5),
//! unavailable entries (S6) and the startup record (S7).

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::state_file;
use super::send_tests::{Server, serve};
use super::workspace_picker::TestPicker;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::write;

pub(super) const FIRST_ID: &str = "01J00000000000000000000A01";
const SECOND_ID: &str = "01J00000000000000000000B01";
const FIRST_REQUEST: &str = "01J00000000000000000000A03";
const SECOND_REQUEST: &str = "01J00000000000000000000B03";

pub(super) fn temp_dir() -> TempDir {
    match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    }
}

/// A workspace `name` with id `id`, one collection and the request `request` at `url`, and
/// the environment `dev`.
pub(super) fn workspace(id: &str, name: &str, request: &str, url: &str) -> TempDir {
    let dir = temp_dir();
    let root = dir.path();
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{id}\"\nname = \"{name}\"\nformat = 1\n"),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"API {name}\"\n", collection_of(id)),
    );
    write(
        root,
        "collections/api/req.req.toml",
        &format!("id = \"{request}\"\nname = \"Req {name}\"\nmethod = \"GET\"\nurl = \"{url}\"\n"),
    );
    write(
        root,
        "environments/dev.env.toml",
        &format!("id = \"{}\"\nname = \"dev\"\n", sibling(id, '4')),
    );
    dir
}

/// `id` with its last character replaced by `last`.
pub(super) fn sibling(id: &str, last: char) -> String {
    let mut sibling: String = id.chars().take(id.len() - 1).collect();
    sibling.push(last);
    sibling
}

fn collection_of(id: &str) -> String {
    sibling(id, '2')
}

pub(super) fn canonical(path: &Path) -> PathBuf {
    match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => panic!("canonicalize {}: {error}", path.display()),
    }
}

pub(super) fn open_state(path: &Path) -> AppState {
    match AppState::open(path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    }
}

/// Records `dir` as recent at `at` in the state at `state_path`.
fn record(state_path: &Path, dir: &Path, id: &str, name: &str, at: i64) {
    let mut state = open_state(state_path);
    if let Err(error) = state.record_recent_workspace(&canonical(dir), id, name, at) {
        panic!("record: {error}");
    }
}

pub(super) fn recents(state_path: &Path) -> Vec<(PathBuf, String)> {
    match open_state(state_path).recent_workspaces() {
        Ok(found) => found
            .into_iter()
            .map(|recent| (recent.path, recent.name))
            .collect(),
        Err(error) => panic!("recents: {error}"),
    }
}

/// The shell on `dir` (or none) over the state at `state_path`, with `picker`.
pub(super) fn render<'a>(
    cx: &'a mut TestAppContext,
    dir: Option<&Path>,
    state_path: &Path,
    picker: TestPicker,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    cx.executor().allow_parking();
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    cx.update(super::bind_keys);
    let state = open_state(state_path);
    let workspace = OpenedWorkspace::open(dir.map(Path::to_path_buf));
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = Shell::with_state(workspace, Some(state), cx);
        shell.picker = Rc::new(picker);
        shell
    });
    cx.run_until_parked();
    (shell, cx)
}

pub(super) fn bounds(
    cx: &mut VisualTestContext,
    selector: &str,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
}

pub(super) fn present(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    match bounds(cx, selector) {
        Some(found) => found,
        None => panic!("{selector} is missing"),
    }
}

pub(super) fn click(cx: &mut VisualTestContext, selector: &str) {
    let target = present(cx, selector);
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

pub(super) fn name(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| shell.read(cx).workspace.name().map(str::to_owned))
}

fn open_ids(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<String> {
    cx.update(|_, cx| shell.read(cx).documents.open_ids().to_vec())
}

fn drafted(cx: &mut VisualTestContext, shell: &Entity<Shell>, id: &str) -> bool {
    cx.update(|_, cx| shell.read(cx).drafts.contains_key(id))
}

/// Two workspaces, both recent, `First` older; the window opens on `First`.
pub(super) struct Pair {
    pub(super) first: TempDir,
    pub(super) second: TempDir,
    pub(super) _state_dir: TempDir,
    pub(super) state_path: PathBuf,
}

pub(super) fn pair(url: &str) -> Pair {
    let first = workspace(FIRST_ID, "First", FIRST_REQUEST, url);
    let second = workspace(SECOND_ID, "Second", SECOND_REQUEST, "https://second/x");
    let (state_dir, state_path) = state_file();
    record(&state_path, second.path(), SECOND_ID, "Second", 1);
    Pair {
        first,
        second,
        _state_dir: state_dir,
        state_path,
    }
}

#[gpui::test]
fn the_startup_workspace_is_recorded(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, _cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    assert_eq!(
        recents(&pair.state_path),
        [
            (canonical(pair.first.path()), "First".to_owned()),
            (canonical(pair.second.path()), "Second".to_owned()),
        ]
    );
}

#[gpui::test]
fn the_menu_lists_recents_newest_first_with_the_current_marked(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (_shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    assert!(bounds(cx, "workspace-menu").is_none());
    click(cx, "workspace-selector");
    let menu = present(cx, "workspace-menu");
    let one = present(cx, "workspace-recent-1");
    let two = present(cx, "workspace-recent-2");
    assert!(menu.contains(&one.origin) && one.top() < two.top());
    for id in [
        "workspace-open",
        "workspace-new",
        "workspace-new-folder",
        "workspace-close",
    ] {
        present(cx, id);
    }
    present(cx, "workspace-recent-current-1");
    assert!(bounds(cx, "workspace-recent-current-2").is_none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "workspace-menu").is_none());
}

#[gpui::test]
fn switching_from_recents_swaps_the_tree_and_keeps_drafts(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, &format!("tree-row-{}", collection_of(FIRST_ID)));
    click(cx, &format!("tree-row-{FIRST_REQUEST}"));
    shell.update(cx, |shell, cx| {
        shell.set_draft_url(FIRST_REQUEST, "https://edited/x".to_owned(), cx);
    });
    assert!(drafted(cx, &shell, FIRST_REQUEST));

    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert!(bounds(cx, "workspace-menu").is_none());
    assert_eq!(name(cx, &shell).as_deref(), Some("Second"));
    assert!(open_ids(cx, &shell).is_empty());
    assert!(!drafted(cx, &shell, FIRST_REQUEST));
    present(cx, &format!("tree-row-{}", collection_of(SECOND_ID)));
    assert!(bounds(cx, &format!("tree-row-{}", collection_of(FIRST_ID))).is_none());
    assert_eq!(
        recents(&pair.state_path).first(),
        Some(&(canonical(pair.second.path()), "Second".to_owned()))
    );

    // `First` is now second in the list.
    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    assert!(drafted(cx, &shell, FIRST_REQUEST));
}

#[gpui::test]
fn a_send_in_flight_is_cancelled_on_switch(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let pair = pair(&format!("http://127.0.0.1:{port}/ping"));
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, &format!("tree-row-{}", collection_of(FIRST_ID)));
    click(cx, &format!("tree-row-{FIRST_REQUEST}"));
    click(cx, "request-send");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let token = loop {
        if let Some(token) =
            cx.update(|_, cx| shell.read(cx).sends.get(FIRST_REQUEST)?.cancel_token())
        {
            break token;
        }
        assert!(std::time::Instant::now() < deadline, "the send never ran");
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.run_until_parked();
    };
    assert!(!token.is_cancelled());
    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert!(token.is_cancelled());
    assert!(cx.update(|_, cx| shell.read(cx).sends.is_empty()));
}

#[gpui::test]
fn open_folder_opens_and_records_the_picked_directory(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let picker = TestPicker {
        folder: Some(pair.second.path().to_path_buf()),
        new: None,
    };
    let (shell, cx) = render(cx, Some(pair.first.path()), &pair.state_path, picker);
    // Forget `Second`, so the record comes from Open folder.
    let mut state = open_state(&pair.state_path);
    if let Err(error) = state.remove_recent_workspace(&canonical(pair.second.path())) {
        panic!("remove: {error}");
    }
    click(cx, "workspace-selector");
    click(cx, "workspace-open");
    assert_eq!(name(cx, &shell).as_deref(), Some("Second"));
    assert_eq!(
        recents(&pair.state_path).first(),
        Some(&(canonical(pair.second.path()), "Second".to_owned()))
    );
}

#[gpui::test]
fn new_workspace_creates_one_named_after_the_directory(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let parent = temp_dir();
    let target = parent.path().join("payments-api");
    let picker = TestPicker {
        folder: None,
        new: Some(target.clone()),
    };
    let (shell, cx) = render(cx, Some(pair.first.path()), &pair.state_path, picker);
    click(cx, "workspace-selector");
    click(cx, "workspace-new-folder");
    assert!(target.join("reqflux.toml").is_file());
    assert_eq!(name(cx, &shell).as_deref(), Some("payments-api"));
    assert_eq!(
        recents(&pair.state_path).first(),
        Some(&(canonical(&target), "payments-api".to_owned()))
    );
}

#[gpui::test]
fn close_shows_the_empty_state_and_a_recent_reopens(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "workspace-selector");
    click(cx, "workspace-close");
    assert_eq!(name(cx, &shell), None);
    for id in [
        "empty-open",
        "empty-new",
        "empty-new-folder",
        "empty-recent-1",
        "empty-recent-2",
        "document-message",
    ] {
        present(cx, id);
    }
    click(cx, "workspace-selector");
    assert!(bounds(cx, "workspace-close").is_none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    click(cx, "empty-recent-2");
    assert_eq!(name(cx, &shell).as_deref(), Some("Second"));
    assert!(bounds(cx, "empty-open").is_none());
}

#[gpui::test]
fn a_deleted_recent_shows_its_error_and_can_be_removed(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let Pair {
        first,
        second,
        _state_dir,
        state_path,
    } = pair;
    let (shell, cx) = render(cx, Some(first.path()), &state_path, TestPicker::default());
    drop(second);
    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
    present(cx, "workspace-menu");
    present(cx, "workspace-error");

    click(cx, "workspace-recent-remove-2");
    assert!(bounds(cx, "workspace-recent-2").is_none());
    assert_eq!(recents(&state_path).len(), 1);
    assert_eq!(name(cx, &shell).as_deref(), Some("First"));
}

#[gpui::test]
fn the_environment_menu_tree_and_variables_work_after_a_switch(cx: &mut TestAppContext) {
    let pair = pair("https://first/x");
    let (shell, cx) = render(
        cx,
        Some(pair.first.path()),
        &pair.state_path,
        TestPicker::default(),
    );
    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).active_environment.clone())
            .as_deref(),
        Some("dev")
    );

    click(cx, "workspace-selector");
    click(cx, "workspace-recent-2");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).active_environment.clone()),
        None
    );
    click(cx, "environment-selector");
    click(cx, "environment-option-dev");
    assert_eq!(
        cx.update(|_, cx| shell.read(cx).active_environment.clone())
            .as_deref(),
        Some("dev")
    );
    click(cx, &format!("tree-row-{}", collection_of(SECOND_ID)));
    click(cx, &format!("tree-row-{SECOND_REQUEST}"));
    assert_eq!(open_ids(cx, &shell), [SECOND_REQUEST]);

    click(cx, "rail-environments");
    present(cx, "variables-scope-globals");
}
