//! In-app approvals (mcp-approvals S3, S4): requests from a client over the window's socket
//! show as a dialog, queue, are answered or withdrawn, and the automation bridge cannot
//! answer them.

use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_mcp::{AppId, BridgeConfig};
use reqflux_engine::approvals::{
    self, ApprovalDecision, ApprovalRequest, AskError, PROTOCOL_VERSION, WorkspaceRef,
};
use tempfile::TempDir;

use super::Shell;
use super::approvals::POLL;
use super::draft_tests::{bounds, click, present, render, state_file};
use super::send_tests::{self, PING, Server, serve, settle};
use crate::workspace_tests::demo_workspace;

type Answer = JoinHandle<Result<ApprovalDecision, AskError>>;

/// Starts the shell's approval listener in a temporary runtime dir, held for the test.
fn listen(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> TempDir {
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

fn socket(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> PathBuf {
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
    }
}

/// Asks the window on its own thread, as `reqflux mcp` does, waiting up to `limit`.
fn ask(socket: PathBuf, request: ApprovalRequest, limit: Duration) -> Answer {
    thread::spawn(move || approvals::ask(&socket, &request, limit))
}

/// Runs the shell's poll until `done` holds, or fails after 10 s.
fn wait_for(
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

fn pending(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> usize {
    cx.update(|_, cx| shell.read(cx).approvals.pending())
}

/// Waits for the client thread's answer while the shell keeps running.
fn answer(cx: &mut VisualTestContext, client: Answer) -> Result<ApprovalDecision, AskError> {
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

fn label(automation: &gpui_mcp::Automation, id: &str) -> Option<String> {
    automation
        .snapshot()
        .nodes
        .get(id)
        .and_then(|node| node.label.clone())
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
fn a_request_shows_the_dialog_with_its_text_and_the_chip(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    assert!(bounds(cx, "approval-dialog").is_none());
    assert_eq!(label(&automation, "status-approvals"), None);

    let client = ask(
        socket(cx, &shell),
        request(1, "request_execute"),
        Duration::from_secs(30),
    );
    wait_for(cx, &shell, "the request arrives", |shell| {
        shell.approvals.pending() == 1
    });
    present(cx, "approval-dialog");
    for id in ["approval-allow", "approval-allow-session", "approval-deny"] {
        present(cx, id);
    }
    for (id, text) in [
        ("approval-tool", "request_execute"),
        ("approval-class", "execute"),
        ("approval-workspace", "Demo"),
        ("approval-targets", "Ping, API"),
        ("approval-method", "POST"),
        ("approval-url", "https://example.test/?key=*****"),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(text), "{id}");
    }
    assert_eq!(
        label(&automation, "status-approvals").as_deref(),
        Some("1 approval pending")
    );

    click(cx, "approval-allow");
    assert_eq!(answer(cx, client).ok(), Some(ApprovalDecision::AllowOnce));
    assert!(bounds(cx, "approval-dialog").is_none());
    assert_eq!(label(&automation, "status-approvals"), None);
}

#[gpui::test]
fn requests_queue_and_each_button_sends_its_decision(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    let socket = socket(cx, &shell);
    let limit = Duration::from_secs(30);
    let first = ask(socket.clone(), request(1, "request_execute"), limit);
    wait_for(cx, &shell, "the first request", |shell| {
        shell.approvals.pending() == 1
    });
    let second = ask(socket.clone(), request(2, "item_rename"), limit);
    wait_for(cx, &shell, "the second request", |shell| {
        shell.approvals.pending() == 2
    });
    let third = ask(socket, request(3, "item_delete"), limit);
    wait_for(cx, &shell, "the third request", |shell| {
        shell.approvals.pending() == 3
    });
    assert_eq!(
        label(&automation, "status-approvals").as_deref(),
        Some("3 approvals pending")
    );
    assert_eq!(
        label(&automation, "approval-tool").as_deref(),
        Some("request_execute")
    );
    assert!(bounds(cx, "approval-method").is_some());

    click(cx, "approval-allow-session");
    assert_eq!(answer(cx, first).ok(), Some(ApprovalDecision::AllowSession));
    assert_eq!(
        label(&automation, "approval-tool").as_deref(),
        Some("item_rename")
    );
    assert!(
        bounds(cx, "approval-method").is_none(),
        "only request_execute has a method"
    );
    assert!(bounds(cx, "approval-url").is_none());

    click(cx, "approval-deny");
    assert_eq!(answer(cx, second).ok(), Some(ApprovalDecision::Deny));
    click(cx, "approval-allow");
    assert_eq!(answer(cx, third).ok(), Some(ApprovalDecision::AllowOnce));
    assert_eq!(pending(cx, &shell), 0);
}

#[gpui::test]
fn escape_denies_the_request_shown(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    let client = ask(
        socket(cx, &shell),
        request(1, "item_delete"),
        Duration::from_secs(30),
    );
    wait_for(cx, &shell, "the request", |shell| {
        shell.approvals.pending() == 1
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(answer(cx, client).ok(), Some(ApprovalDecision::Deny));
    assert_eq!(pending(cx, &shell), 0);
}

#[gpui::test]
fn escape_denies_before_it_closes_the_palette(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    cx.update(|window, cx| {
        let focus = shell.read(cx).focus.clone();
        focus.focus(window, cx);
    });
    cx.simulate_keystrokes("ctrl-k");
    cx.run_until_parked();
    let client = ask(
        socket(cx, &shell),
        request(1, "item_delete"),
        Duration::from_secs(30),
    );
    wait_for(cx, &shell, "the request", |shell| {
        shell.approvals.pending() == 1
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(answer(cx, client).ok(), Some(ApprovalDecision::Deny));
    assert!(cx.update(|_, cx| shell.read(cx).palette.is_some()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| shell.read(cx).palette.is_none()));
}

#[gpui::test]
fn a_request_whose_client_timed_out_leaves_the_queue(cx: &mut TestAppContext) {
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
    assert!(matches!(answer(cx, gone), Err(AskError::NoAnswer(_))));
    wait_for(cx, &shell, "the timed-out request leaves", |shell| {
        shell.approvals.pending() == 1
    });
    let shown = cx.update(|_, cx| shell.read(cx).approvals.shown().map(|r| r.id));
    assert_eq!(shown, Some(2));
    click(cx, "approval-deny");
    assert_eq!(answer(cx, waiting).ok(), Some(ApprovalDecision::Deny));
    assert!(bounds(cx, "approval-dialog").is_none());
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
    let allow = present(cx, "approval-allow").center();
    assert_eq!(bridge_descriptor(endpoints.path()), None);
    for operation in click_at(f32::from(allow.x), f32::from(allow.y)) {
        assert!(drive(cx, &before, operation).is_err());
    }
    let escape = serde_json::json!({ "kind": "input",
        "command": { "kind": "key", "keystroke": "escape" } });
    assert!(drive(cx, &before, escape).is_err());
    for _ in 0..5 {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }
    assert_eq!(pending(cx, &shell), 1, "the request stays pending");
    assert!(!client.is_finished());

    click(cx, "approval-deny");
    assert_eq!(answer(cx, client).ok(), Some(ApprovalDecision::Deny));
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
