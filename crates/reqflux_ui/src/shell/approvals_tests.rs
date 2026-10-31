//! In-app approvals (mcp-approvals S3, S4): requests from a client over the window's socket
//! are counted in the status bar, the bell and the rail, listed in the MCP approvals view,
//! answered or withdrawn there, and the automation bridge cannot answer them.

use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_mcp::{AppId, BridgeConfig};
use reqflux_engine::approvals::{
    self, ApprovalDecision, ApprovalRequest, ApprovalResponse, AskError, PROTOCOL_VERSION,
    WorkspaceRef,
};
use tempfile::TempDir;

use super::Shell;
use super::approvals::POLL;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::{self, PING, Server, serve, settle};
use crate::workspace_tests::demo_workspace;

type Answer = JoinHandle<Result<ApprovalResponse, AskError>>;

/// Starts the shell's approval listener in a temporary runtime dir, held for the test.
pub(super) fn listen(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> TempDir {
    let runtime = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("runtime dir: {error}"),
    };
    let dir = runtime.path().join("reqflux");
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.listen_for_approvals(
                |handler| approvals::serve_in(&dir, std::process::id(), handler),
                window,
                cx,
            );
        });
    });
    runtime
}

pub(super) fn socket(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> PathBuf {
    match cx.update(|_, cx| shell.read(cx).approvals.socket().map(Path::to_path_buf)) {
        Some(socket) => socket,
        None => panic!("the window does not listen"),
    }
}

fn request(id: u64, tool: &str) -> ApprovalRequest {
    let execute = tool == "request_execute";
    ApprovalRequest {
        version: PROTOCOL_VERSION,
        id,
        tool: tool.to_owned(),
        class: if execute { "execute" } else { "write" }.to_owned(),
        workspace: Some(WorkspaceRef {
            id: "01W".to_owned(),
            name: "Demo".to_owned(),
        }),
        targets: vec!["Ping".to_owned(), "API".to_owned()],
        method: execute.then(|| "POST".to_owned()),
        url: execute.then(|| "https://example.test/?key=*****".to_owned()),
        ..ApprovalRequest::default()
    }
}

/// Asks the window on its own thread, as `reqflux mcp` does, waiting up to `limit`.
pub(super) fn ask(socket: PathBuf, request: ApprovalRequest, limit: Duration) -> Answer {
    thread::spawn(move || approvals::ask(&socket, &request, limit))
}

/// Runs the shell's poll until `done` holds, or fails after 10 s.
pub(super) fn wait_for(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    what: &str,
    done: impl Fn(&Shell) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
        if cx.update(|_, cx| done(shell.read(cx))) {
            return;
        }
        assert!(Instant::now() < deadline, "never: {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn pending(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> usize {
    cx.update(|_, cx| shell.read(cx).approvals.pending())
}

/// Waits for the client thread's answer while the shell keeps running.
pub(super) fn answer(
    cx: &mut VisualTestContext,
    client: Answer,
) -> Result<ApprovalResponse, AskError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "the client never got an answer");
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(10));
    }
    match client.join() {
        Ok(answer) => answer,
        Err(_) => panic!("the client panicked"),
    }
}

pub(super) fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
}

/// The decision the client got, if it got one.
pub(super) fn decision(cx: &mut VisualTestContext, client: Answer) -> Option<ApprovalDecision> {
    answer(cx, client).ok().map(|response| response.decision)
}

/// Gives the focus to the shell, as the window has it before anything else is focused.
fn focus_shell(cx: &mut VisualTestContext, shell: &Entity<Shell>) {
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
}

fn shell_focused(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> bool {
    cx.update(|window, cx| shell.read(cx).focus.is_focused(window))
}

/// The request id of the selected call of the view.
fn selected(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<u64> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .approvals
            .selected()
            .map(|request| request.id)
    })
}

/// Asks the window and waits until it has `count` pending requests.
pub(super) fn arrive(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    request: ApprovalRequest,
    count: usize,
) -> Answer {
    let client = ask(socket(cx, shell), request, Duration::from_secs(30));
    wait_for(cx, shell, "the request arrives", |shell| {
        shell.approvals.pending() == count
    });
    client
}

/// A shell on the demo workspace that listens for approvals.
fn window(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Vec<TempDir>) {
    let dir = demo_workspace();
    let (state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    let runtime = listen(cx, &shell);
    (shell, cx, vec![dir, state_dir, runtime])
}

#[gpui::test]
fn a_request_is_counted_without_opening_anything(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    focus_shell(cx, &shell);
    assert_eq!(label(&automation, "approvals-badge"), None);
    assert_eq!(label(&automation, "rail-approvals-badge"), None);

    let first = arrive(cx, &shell, request(1, "request_execute"), 1);
    assert_eq!(label(&automation, "approvals-badge").as_deref(), Some("1"));
    assert_eq!(
        label(&automation, "rail-approvals-badge").as_deref(),
        Some("1")
    );
    assert!(bounds(cx, "approvals-view").is_none(), "nothing opens");
    assert!(bounds(cx, "approval-dialog").is_none(), "no modal");
    assert!(bounds(cx, "approval-backdrop").is_none(), "no backdrop");
    assert!(shell_focused(cx, &shell), "the focus stays where it was");

    let second = arrive(cx, &shell, request(2, "item_rename"), 2);
    assert_eq!(label(&automation, "approvals-badge").as_deref(), Some("2"));
    assert_eq!(
        label(&automation, "rail-approvals-badge").as_deref(),
        Some("2")
    );

    // Escape no longer answers anything.
    cx.simulate_keystrokes("escape");
    for _ in 0..3 {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }
    assert_eq!(pending(cx, &shell), 2);
    assert!(!first.is_finished() && !second.is_finished());

    click(cx, "rail-approvals");
    click(cx, "approval-allow");
    assert_eq!(decision(cx, first), Some(ApprovalDecision::AllowOnce));
    click(cx, "approval-allow");
    assert_eq!(decision(cx, second), Some(ApprovalDecision::AllowOnce));
    assert_eq!(label(&automation, "approvals-badge"), None);
    assert_eq!(label(&automation, "rail-approvals-badge"), None);
}

#[gpui::test]
fn the_rail_opens_the_view_with_the_list_and_the_detail(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    click(cx, "rail-approvals");
    present(cx, "approvals-view");
    assert_eq!(
        label(&automation, "approvals-empty").as_deref(),
        Some("No agent call is waiting")
    );
    assert_eq!(
        label(&automation, "approvals-pending-label").as_deref(),
        Some("Pending · 0")
    );
    assert!(
        label(&automation, "approvals-footer").is_some_and(|text| text.contains("audit log")),
        "the footer names the audit log"
    );

    let client = arrive(cx, &shell, request(1, "request_execute"), 1);
    assert!(bounds(cx, "approvals-empty").is_none());
    present(cx, "approvals-list");
    assert_eq!(
        label(&automation, "approvals-pending-label").as_deref(),
        Some("Pending · 1")
    );
    let row = label(&automation, "approvals-row-1").unwrap_or_default();
    assert!(row.contains("request_execute"), "{row}");
    assert!(row.contains("an agent · waiting just now"), "{row}");
    for (id, text) in [
        ("approval-tool", "request_execute"),
        ("approval-class", "Execute"),
        ("approval-targets", "Ping, API"),
        ("approval-method", "POST"),
        ("approval-url", "https://example.test/?key=*****"),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(text), "{id}");
    }
    for id in [
        "approval-reason",
        "approval-deny",
        "approval-allow-session",
        "approval-allow",
    ] {
        present(cx, id);
    }

    click(cx, "approval-allow");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::AllowOnce));
    present(cx, "approvals-empty");
}

#[gpui::test]
fn the_bell_the_palette_and_ctrl_shift_m_open_the_view(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let client = arrive(cx, &shell, request(1, "item_delete"), 1);
    focus_shell(cx, &shell);
    assert!(
        bounds(cx, "status-approvals").is_none(),
        "the count stays on the bell and the rail, not the status bar"
    );

    click(cx, "approvals");
    present(cx, "approvals-view");
    present(cx, "approval-tool");
    cx.simulate_keystrokes("ctrl-1");
    cx.run_until_parked();
    assert!(bounds(cx, "approvals-view").is_none());

    cx.simulate_keystrokes("ctrl-shift-m");
    cx.run_until_parked();
    present(cx, "approvals-view");
    cx.simulate_keystrokes("ctrl-1");
    cx.run_until_parked();
    assert!(bounds(cx, "approvals-view").is_none());

    cx.simulate_keystrokes("ctrl-k");
    cx.run_until_parked();
    cx.simulate_input("Open MCP approvals");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    present(cx, "approvals-view");

    click(cx, "approval-deny");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::Deny));
}

#[gpui::test]
fn each_button_sends_its_decision_and_a_denial_its_reason(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let first = arrive(cx, &shell, request(1, "request_execute"), 1);
    let second = arrive(cx, &shell, request(2, "item_rename"), 2);
    let third = arrive(cx, &shell, request(3, "item_delete"), 3);
    click(cx, "rail-approvals");

    click(cx, "approval-allow-session");
    assert_eq!(decision(cx, first), Some(ApprovalDecision::AllowSession));

    click(cx, "approval-reason");
    cx.simulate_input("not on prod");
    cx.run_until_parked();
    click(cx, "approval-deny");
    let denied = answer(cx, second).ok();
    assert_eq!(
        denied.as_ref().map(|r| r.decision),
        Some(ApprovalDecision::Deny)
    );
    assert_eq!(
        denied.and_then(|response| response.reason).as_deref(),
        Some("not on prod")
    );

    // The reason field is empty again for the next call: a bare denial has no reason.
    click(cx, "approval-deny");
    let bare = answer(cx, third).ok();
    assert_eq!(
        bare.as_ref().map(|r| r.decision),
        Some(ApprovalDecision::Deny)
    );
    assert_eq!(bare.and_then(|response| response.reason), None);
    assert_eq!(pending(cx, &shell), 0);
}

#[gpui::test]
fn calls_list_longest_wait_first_and_keys_move_allow_and_deny(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    let first = arrive(cx, &shell, request(1, "request_execute"), 1);
    let second = arrive(cx, &shell, request(2, "item_rename"), 2);
    let third = arrive(cx, &shell, request(3, "item_delete"), 3);
    click(cx, "rail-approvals");
    for (n, tool) in [
        (1, "request_execute"),
        (2, "item_rename"),
        (3, "item_delete"),
    ] {
        let row = label(&automation, &format!("approvals-row-{n}")).unwrap_or_default();
        assert!(row.contains(tool), "row {n}: {row}");
    }
    assert_eq!(
        selected(cx, &shell),
        Some(1),
        "the longest wait is selected"
    );

    cx.simulate_keystrokes("j");
    assert_eq!(selected(cx, &shell), Some(2));
    cx.simulate_keystrokes("down");
    assert_eq!(selected(cx, &shell), Some(3));
    cx.simulate_keystrokes("k");
    assert_eq!(selected(cx, &shell), Some(2));
    cx.simulate_keystrokes("up");
    assert_eq!(selected(cx, &shell), Some(1));
    click(cx, "approvals-row-2");
    assert_eq!(selected(cx, &shell), Some(2));
    assert_eq!(
        label(&automation, "approval-tool").as_deref(),
        Some("item_rename")
    );

    cx.simulate_keystrokes("a");
    assert_eq!(decision(cx, second), Some(ApprovalDecision::AllowOnce));
    assert_eq!(selected(cx, &shell), Some(3), "answering selects the next");

    // `d` typed in the reason field is text, not a denial.
    click(cx, "approval-reason");
    cx.simulate_keystrokes("d");
    for _ in 0..3 {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }
    assert_eq!(pending(cx, &shell), 2);

    click(cx, "approvals-row-1");
    cx.simulate_keystrokes("d");
    assert_eq!(decision(cx, first), Some(ApprovalDecision::Deny));
    assert_eq!(selected(cx, &shell), Some(3));
    click(cx, "approval-allow");
    assert_eq!(decision(cx, third), Some(ApprovalDecision::AllowOnce));
}

#[gpui::test]
fn a_request_whose_client_timed_out_leaves_the_list(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let socket = socket(cx, &shell);
    let gone = ask(
        socket.clone(),
        request(1, "request_execute"),
        Duration::from_millis(800),
    );
    wait_for(cx, &shell, "the first request", |shell| {
        shell.approvals.pending() == 1
    });
    let waiting = ask(socket, request(2, "item_rename"), Duration::from_secs(30));
    wait_for(cx, &shell, "the second request", |shell| {
        shell.approvals.pending() == 2
    });
    click(cx, "rail-approvals");
    present(cx, "approvals-row-2");
    assert!(matches!(answer(cx, gone), Err(AskError::NoAnswer(_))));
    wait_for(cx, &shell, "the timed-out request leaves", |shell| {
        shell.approvals.pending() == 1
    });
    assert!(bounds(cx, "approvals-row-2").is_none());
    assert_eq!(selected(cx, &shell), Some(2));
    click(cx, "approval-deny");
    assert_eq!(decision(cx, waiting), Some(ApprovalDecision::Deny));
    present(cx, "approvals-empty");
}

/// The descriptor the bridge advertises in `dir`, if any.
fn bridge_descriptor(dir: &Path) -> Option<serde_json::Value> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// One framed call to the bridge `descriptor` names, the way a gpui-mcp client makes it.
fn bridge_call(
    descriptor: &serde_json::Value,
    operation: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = descriptor["endpoint"]["path"]
        .as_str()
        .ok_or("the descriptor has no socket")?;
    let request = serde_json::json!({
        "protocol_version": descriptor["protocol_version"],
        "request_id": 1,
        "token": descriptor["token"],
        "operation": operation,
    });
    let body = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    let mut stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| error.to_string())?;
    let length = u32::try_from(body.len()).map_err(|error| error.to_string())?;
    stream
        .write_all(&length.to_be_bytes())
        .and_then(|()| stream.write_all(&body))
        .map_err(|error| error.to_string())?;
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| error.to_string())?;
    let mut body = vec![0_u8; u32::from_be_bytes(length) as usize];
    stream
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    let response: serde_json::Value =
        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    match response.get("error") {
        Some(error) if !error.is_null() => Err(error.to_string()),
        _ => Ok(response),
    }
}

/// Makes `operation` on a thread while the window keeps running, as a client would.
fn drive(
    cx: &mut VisualTestContext,
    descriptor: &serde_json::Value,
    operation: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let descriptor = descriptor.clone();
    let call = thread::spawn(move || bridge_call(&descriptor, operation));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !call.is_finished() {
        assert!(Instant::now() < deadline, "the bridge call never ended");
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
    match call.join() {
        Ok(result) => result,
        Err(_) => panic!("the bridge call panicked"),
    }
}

fn click_at(x: f32, y: f32) -> [serde_json::Value; 2] {
    let at = serde_json::json!({ "x": x, "y": y });
    [
        serde_json::json!({ "kind": "pointer_input", "command": {
            "kind": "mouse_down", "point": at, "button": "left", "click_count": 1 } }),
        serde_json::json!({ "kind": "pointer_input", "command": {
            "kind": "mouse_up", "point": at, "button": "left", "click_count": 1 } }),
    ]
}

#[gpui::test]
fn the_automation_bridge_cannot_answer_a_pending_request(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let endpoints = match TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("endpoint dir: {error}"),
    };
    let config = AppId::new("reqflux")
        .ok()
        .map(|app_id| BridgeConfig::new(app_id, "ReqFlux"))
        .and_then(|config| config.endpoint_dir(endpoints.path()).ok());
    let Some(config) = config else {
        panic!("bridge config");
    };
    cx.update(|window, cx| crate::ui_automation::install_with(window, cx, config));
    cx.run_until_parked();
    let Some(before) = bridge_descriptor(endpoints.path()) else {
        panic!("the bridge advertises no descriptor");
    };
    // The client works: it reaches the bridge with no approval pending.
    assert!(drive(cx, &before, serde_json::json!({ "kind": "ping" })).is_ok());

    let client = ask(
        socket(cx, &shell),
        request(1, "request_execute"),
        Duration::from_secs(30),
    );
    wait_for(cx, &shell, "the request", |shell| {
        shell.approvals.pending() == 1
    });
    click(cx, "rail-approvals");
    let allow = present(cx, "approval-allow").center();
    assert_eq!(bridge_descriptor(endpoints.path()), None);
    for operation in click_at(f32::from(allow.x), f32::from(allow.y)) {
        assert!(drive(cx, &before, operation).is_err());
    }
    for keystroke in ["a", "escape"] {
        let key = serde_json::json!({ "kind": "input",
            "command": { "kind": "key", "keystroke": keystroke } });
        assert!(drive(cx, &before, key).is_err(), "{keystroke}");
    }
    for _ in 0..5 {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }
    assert_eq!(pending(cx, &shell), 1, "the request stays pending");
    assert!(!client.is_finished());

    click(cx, "approval-deny");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::Deny));
    let Some(after) = bridge_descriptor(endpoints.path()) else {
        panic!("the bridge did not come back");
    };
    assert!(drive(cx, &after, serde_json::json!({ "kind": "ping" })).is_ok());
}

#[gpui::test]
fn the_palette_and_the_workspace_menu_close_on_escape_with_no_approval_pending(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _dirs) = window(cx);
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| shell.read(cx).palette.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| shell.read(cx).palette.is_none()));

    click(cx, "workspace-selector");
    present(cx, "workspace-menu");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "workspace-menu").is_none());
}

#[gpui::test]
fn the_environment_menu_and_escape_cancel_work_with_no_approval_pending(cx: &mut TestAppContext) {
    let (shell, cx) = send_tests::render(cx, serve(Server::Silent));
    let _runtime = listen(cx, &shell);
    click(cx, "environment-selector");
    present(cx, "environment-menu");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "environment-menu").is_none());

    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
}
