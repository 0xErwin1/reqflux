//! The run document (v1-features S1): opening its setup, running a collection or folder on
//! a background thread with events into the grid, Stop, Stop on first failure, the history
//! the run records, the script writes it persists as current values and the selected
//! step's result (design/runner/01-run-document).

use std::thread;
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_engine::SecretScope;
use reqflux_engine::state::AppState;
use tempfile::TempDir;

use super::Shell;
use super::model::{Phase, RowStatus};
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::{Server, closed_port, serve};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000701";
const FOLDER: &str = "01J00000000000000000000702";
const FIRST: &str = "01J00000000000000000000703";
const SECOND: &str = "01J00000000000000000000704";
const WORKSPACE: &str = "01J00000000000000000000900";

/// Passes on a 200 and sets the global `token` for the next request.
const FIRST_TEST: &str = "pm.test(\"status is 200\", function () {\n  \
                          pm.expect(pm.response.code).to.eql(200);\n});\n\
                          pm.globals.set(\"token\", \"t-1\");\n";
/// Fails whatever the response.
const FAILING_TEST: &str =
    "pm.test(\"wrong code\", function () {\n  pm.expect(200, \"bad code\").to.eql(404);\n});\n";

/// A workspace whose collection `API` holds `GET First` on `first` and, in the folder
/// `later`, `POST Second` on `second`; `test` is the test script of First.
fn workspace(first: u16, second: u16, test: &str) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\norder = [\"first\", \"later\"]\n"),
    );
    write(
        root,
        "collections/api/first.req.toml",
        &format!(
            "id = \"{FIRST}\"\nname = \"First\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{first}/first\"\n"
        ),
    );
    write(root, "collections/api/first.test.js", test);
    write(
        root,
        "collections/api/later/folder.toml",
        &format!("id = \"{FOLDER}\"\nname = \"later\"\n"),
    );
    write(
        root,
        "collections/api/later/second.req.toml",
        &format!(
            "id = \"{SECOND}\"\nname = \"Second\"\nmethod = \"POST\"\n\
             url = \"http://127.0.0.1:{second}/second\"\n"
        ),
    );
    dir
}

fn open_run(cx: &mut VisualTestContext, shell: &Entity<Shell>, target: &str) {
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.open_run(target, cx)));
    cx.run_until_parked();
}

fn phase(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Phase> {
    cx.update(|_, cx| shell.read(cx).run.as_ref().map(|run| run.model.phase))
}

/// Waits until the run ends, advancing the clock the document polls its events on. Only
/// `Finished` is terminal: the start click is handled on a later tick, so a helper that
/// stopped at "not running" would return while the run is still on its setup form and the
/// assertions after it would race the machine's load.
fn settle_run(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        match phase(cx, shell) {
            Some(Phase::Setup | Phase::Running) => {}
            Some(Phase::Finished) | None => return,
        }
        assert!(Instant::now() < deadline, "the run never ended");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Waits until the run has a request in flight.
fn settle_sending(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let sending = cx.update(|_, cx| {
            shell
                .read(cx)
                .run
                .as_ref()
                .is_some_and(|run| run.model.sending.is_some())
        });
        if sending {
            return;
        }
        assert!(Instant::now() < deadline, "the run never sent");
        thread::sleep(Duration::from_millis(10));
    }
}

fn statuses(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Vec<(String, RowStatus)> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .run
            .as_ref()
            .map(|run| {
                run.model
                    .rows()
                    .into_iter()
                    .map(|row| (row.plan.name, row.status))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn title(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .run
            .as_ref()
            .map(|run| run.model.title())
            .unwrap_or_default()
    })
}

fn history_names(state_path: &std::path::Path) -> Vec<String> {
    let state = match AppState::open(state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    match state.history(WORKSPACE, 50) {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| entry.record.request_name)
            .collect(),
        Err(error) => panic!("history: {error}"),
    }
}

#[gpui::test]
fn run_opens_the_setup_in_its_own_active_tab(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{FIRST}"));
    open_run(cx, &shell, COLLECTION);

    assert_eq!(phase(cx, &shell), Some(Phase::Setup));
    assert_eq!(title(cx, &shell), "Run API");
    present(cx, "runner-tab");
    present(cx, "runner-setup");
    present(cx, "runner-environment");
    present(cx, "runner-bail");
    present(cx, "runner-order-1");
    present(cx, "runner-order-2");
    assert!(
        bounds(cx, "runner-order-3").is_none(),
        "two requests to run"
    );
    let active = cx.update(|_, cx| shell.read(cx).run_tab_active());
    assert!(active, "the run tab is the active document");
    assert!(
        bounds(cx, "request-send").is_none(),
        "the request is not drawn under the run"
    );
    assert!(
        history_names(&state_path).is_empty(),
        "opening the setup sends nothing"
    );
}

#[gpui::test]
fn the_order_heads_folders_only_and_root_requests_sit_under_no_header(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);

    assert!(
        bounds(cx, "runner-order-folder-0").is_none(),
        "First sits at the collection root"
    );
    present(cx, "runner-order-folder-1");
}

#[gpui::test]
fn a_step_result_lists_its_unresolved_variables(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    write(
        dir.path(),
        "collections/api/first.req.toml",
        &format!(
            "id = \"{FIRST}\"\nname = \"First\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{port}/first?k={{{{missing}}}}\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_run(cx, &shell);

    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.select_run_step(0, cx)));
    cx.run_until_parked();
    present(cx, "runner-result-unresolved");
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.select_run_step(1, cx)));
    cx.run_until_parked();
    present(cx, "runner-result");
    assert!(
        bounds(cx, "runner-result-unresolved").is_none(),
        "Second resolved everything"
    );
}

#[gpui::test]
fn a_run_sends_every_request_records_history_and_persists_script_writes(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_run(cx, &shell);

    assert_eq!(title(cx, &shell), "API passed");
    assert_eq!(
        statuses(cx, &shell),
        [
            ("First".to_owned(), RowStatus::Done(0)),
            ("Second".to_owned(), RowStatus::Done(1)),
        ]
    );
    present(cx, "runner-grid");
    present(cx, "runner-run-again");
    let tree = crate::shell::toasts::tests::tree(cx);
    let headers: Vec<Option<&str>> = (0..crate::tokens::RUNNER_COLUMNS.len())
        .map(|column| {
            tree.nodes
                .get(&format!("runner-grid-header-cell-{column}"))
                .and_then(|node| node.label.as_deref())
        })
        .collect();
    let names: Vec<Option<&str>> = crate::tokens::RUNNER_COLUMNS
        .iter()
        .map(|(name, _)| Some(*name))
        .collect();
    assert_eq!(headers, names, "the header cells name their columns");
    assert_eq!(history_names(&state_path), ["Second", "First"]);
    let current = cx.update(|_, cx| {
        shell
            .read(cx)
            .current
            .get(SecretScope::Global, "token")
            .map(str::to_owned)
    });
    assert_eq!(
        current.as_deref(),
        Some("t-1"),
        "the window's copy is updated"
    );
    let stored = match AppState::open(&state_path).and_then(|state| state.current_values(WORKSPACE))
    {
        Ok(values) => values.get(SecretScope::Global, "token").map(str::to_owned),
        Err(error) => panic!("current values: {error}"),
    };
    assert_eq!(stored.as_deref(), Some("t-1"), "the write is persisted");
}

#[gpui::test]
fn selecting_a_finished_step_shows_its_tests_and_recorded_response(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_run(cx, &shell);
    assert!(bounds(cx, "runner-result").is_none());
    present(cx, "runner-hint");

    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.select_run_step(0, cx)));
    cx.run_until_parked();
    present(cx, "runner-result");
    present(cx, "runner-test-1");
    present(cx, "runner-response");
    present(cx, "response");
    click(cx, "runner-result-close");
    assert!(bounds(cx, "runner-result").is_none());
}

#[gpui::test]
fn stop_on_first_failure_skips_the_rest(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FAILING_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-bail");
    click(cx, "runner-start");
    settle_run(cx, &shell);

    assert_eq!(title(cx, &shell), "API failed");
    assert_eq!(
        statuses(cx, &shell),
        [
            ("First".to_owned(), RowStatus::Done(0)),
            ("Second".to_owned(), RowStatus::Skipped),
        ]
    );
    assert_eq!(history_names(&state_path), ["First"]);
}

#[gpui::test]
fn a_failed_request_continues_without_stop_on_first_failure(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(closed_port(), port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_run(cx, &shell);

    assert_eq!(title(cx, &shell), "API failed");
    assert_eq!(
        statuses(cx, &shell),
        [
            ("First".to_owned(), RowStatus::Done(0)),
            ("Second".to_owned(), RowStatus::Done(1)),
        ]
    );
}

#[gpui::test]
fn stop_cancels_the_send_in_flight_and_sends_nothing_more(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, COLLECTION);
    click(cx, "runner-start");
    settle_sending(cx, &shell);
    present(cx, "runner-progress");
    present(cx, "runner-progress-fill");
    click(cx, "runner-stop");
    settle_run(cx, &shell);

    assert_eq!(title(cx, &shell), "API stopped");
    assert_eq!(
        statuses(cx, &shell),
        [
            ("First".to_owned(), RowStatus::Done(0)),
            ("Second".to_owned(), RowStatus::Skipped),
        ]
    );
    assert!(
        history_names(&state_path).is_empty(),
        "a cancelled send is not recorded"
    );
}

#[gpui::test]
fn a_folder_runs_only_its_requests(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_run(cx, &shell, FOLDER);
    assert_eq!(title(cx, &shell), "Run later");
    click(cx, "runner-start");
    settle_run(cx, &shell);
    assert_eq!(
        statuses(cx, &shell),
        [("Second".to_owned(), RowStatus::Done(0))]
    );
    assert_eq!(history_names(&state_path), ["Second"]);
}

#[gpui::test]
fn closing_the_run_tab_drops_it_and_a_request_tab_still_activates(cx: &mut TestAppContext) {
    let port = serve(Server::Ok);
    let dir = workspace(port, port, FIRST_TEST);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{FIRST}"));
    open_run(cx, &shell, COLLECTION);

    click(cx, &format!("doc-tab-{FIRST}"));
    let (run_active, active) = cx.update(|_, cx| {
        let shell = shell.read(cx);
        (
            shell.run_tab_active(),
            shell.active_request().map(str::to_owned),
        )
    });
    assert!(!run_active, "activating a request tab leaves the run tab");
    assert_eq!(active.as_deref(), Some(FIRST));
    present(cx, "request-send");
    present(cx, "runner-tab");

    click(cx, "runner-tab-close");
    assert!(bounds(cx, "runner-tab").is_none());
    present(cx, &format!("doc-tab-{FIRST}"));
}
