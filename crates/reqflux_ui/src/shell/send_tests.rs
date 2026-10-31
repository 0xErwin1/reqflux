//! Sending the active request (S1), the Cancel button (S2), cancelling (S3), replacing and
//! closing (S4), against a local HTTP server.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use gpui::{AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext};

use super::Shell;
use super::response::ResponseTab;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000031";
pub(super) const PING: &str = "01J00000000000000000000032";

/// How the test server answers each connection.
#[derive(Clone, Copy)]
pub(super) enum Server {
    /// `200 OK` with the JSON body [`JSON`].
    Ok,
    /// Never answers; the connection stays open until the client drops it.
    Silent,
}

/// The body of [`Server::Ok`].
pub(super) const JSON: &str = r#"{"id":7,"tags":["a"]}"#;

/// Reads the request head off `stream`.
fn read_head(stream: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.extend_from_slice(&byte),
            Ok(_) | Err(_) => return,
        }
    }
}

/// Starts a server on a free local port and returns the port.
pub(super) fn serve(server: Server) -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("bind: {error}"),
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => panic!("address: {error}"),
    };
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            thread::spawn(move || {
                read_head(&mut stream);
                match server {
                    Server::Ok => {
                        let reply = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                             content-length: {}\r\nconnection: close\r\n\r\n{JSON}",
                            JSON.len()
                        );
                        if let Err(error) = stream.write_all(reply.as_bytes()) {
                            eprintln!("write: {error}");
                        }
                    }
                    Server::Silent => {
                        let mut rest = [0_u8; 64];
                        while matches!(stream.read(&mut rest), Ok(read) if read > 0) {}
                    }
                }
            });
        }
    });
    port
}

/// A port nothing listens on.
pub(super) fn closed_port() -> u16 {
    match TcpListener::bind("127.0.0.1:0").and_then(|listener| listener.local_addr()) {
        Ok(address) => address.port(),
        Err(error) => panic!("bind: {error}"),
    }
}

/// Opens a workspace whose request `GET Ping` targets `port` and opens its tab.
pub(super) fn render(
    cx: &mut TestAppContext,
    port: u16,
) -> (Entity<Shell>, &mut VisualTestContext) {
    render_files(cx, port, &[])
}

/// [`render`], with `files` written next to the request as script sidecars.
pub(super) fn render_files<'a>(
    cx: &'a mut TestAppContext,
    port: u16,
    files: &[(&str, &str)],
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    render_in(cx, port, false, files)
}

/// [`render`], with the shell inside gpui-component's `Root` when `rooted`, as the app's
/// window has it, and `files` written next to the request as script sidecars.
fn render_in<'a>(
    cx: &'a mut TestAppContext,
    port: u16,
    rooted: bool,
    files: &[(&str, &str)],
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    // The engine wakes the awaiting task from its own threads.
    cx.executor().allow_parking();
    cx.update(super::bind_keys);
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/ping.req.toml",
        &format!(
            "id = \"{PING}\"\nname = \"Ping\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:{port}/ping\"\n"
        ),
    );
    for (name, content) in files {
        write(root, &format!("collections/api/{name}"), content);
    }
    let workspace = OpenedWorkspace::open(Some(root.to_path_buf()));
    drop(dir);
    let (shell, cx) = if rooted {
        let (root, cx) = cx.add_window_view(|window, cx| {
            let shell = cx.new(|cx| Shell::new(workspace, cx));
            gpui_component::Root::new(shell, window, cx)
        });
        let shell = cx.update(|_, cx| root.read(cx).view().clone().downcast::<Shell>());
        match shell {
            Ok(shell) => (shell, cx),
            Err(_) => panic!("the root view is not the shell"),
        }
    } else {
        cx.add_window_view(|_, cx| Shell::new(workspace, cx))
    };
    cx.run_until_parked();
    click_owned(cx, &format!("tree-row-{PING}"));
    (shell, cx)
}

pub(super) fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let Some(target) = cx.debug_bounds(selector) else {
        panic!("{selector} is missing");
    };
    cx.simulate_click(target.center(), Modifiers::none());
    cx.run_until_parked();
}

pub(super) fn click_owned(cx: &mut VisualTestContext, selector: &str) {
    click(cx, Box::leak(selector.to_owned().into_boxed_str()));
}

/// The response placeholder text of the request `id`.
pub(super) fn response(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    id: &str,
) -> Option<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .sends
            .get(id)
            .and_then(|send| send.state.text())
    })
}

/// Runs the executor until the send of `id` is no longer in flight, or fails after 10 s.
pub(super) fn settle(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    id: &str,
) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let text = response(cx, shell, id);
        if text.as_deref() != Some("Sending") {
            return text;
        }
        assert!(Instant::now() < deadline, "the send of {id} never ended");
        thread::sleep(Duration::from_millis(10));
    }
}

#[gpui::test]
fn clicking_send_shows_the_status(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    assert_eq!(response(cx, &shell, PING), None);
    assert!(cx.debug_bounds("response-status").is_none());

    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    assert!(cx.debug_bounds("response-status").is_some());
    assert!(cx.debug_bounds("request-send").is_some());
}

#[gpui::test]
fn ctrl_enter_sends_the_active_request(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
}

/// A pre-request script that busy-loops for `millis` of wall-clock time.
fn busy_script(millis: u64) -> String {
    format!("const start = Date.now();\nwhile (Date.now() - start < {millis}) {{}}\n")
}

/// S14: a slow pre-request script never blocks the window. Dispatching Ctrl+Enter returns
/// before the script could have finished (preparing runs on GPUI's background executor),
/// the shell still answers while the send is in flight, and the send settles to its
/// response.
///
/// The script busy-loops on wall-clock time, so running it on the UI thread would make the
/// dispatch last at least `SCRIPT_MS` however fast the machine is. Bounding the dispatch by
/// `SCRIPT_MS` itself therefore proves the script ran elsewhere without assuming how quick
/// the rest of the dispatch is; `SCRIPT_MS` stays below the 5 s sandbox time limit.
#[gpui::test]
fn a_slow_pre_request_script_does_not_block_the_window(cx: &mut TestAppContext) {
    const SCRIPT_MS: u64 = 2_000;
    let (shell, cx) = render_files(
        cx,
        serve(Server::Ok),
        &[("ping.pre.js", &busy_script(SCRIPT_MS))],
    );
    let started = Instant::now();
    // Ctrl+Enter dispatches `SendRequest`; `simulate_keystrokes` would run until parked
    // and hide the block, so the action handler is invoked directly: what this measures is
    // the work the key does on the UI thread.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.send_active(window, cx));
    });
    let dispatched = started.elapsed();
    assert!(
        dispatched < Duration::from_millis(SCRIPT_MS),
        "ctrl-enter blocked for {dispatched:?} on the pre-request script"
    );
    // The window answers at once: the send is in flight and a sub-tab can be selected.
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.select_response_tab(PING, ResponseTab::Headers, cx);
        });
    });
    let tab = cx.update(|_, cx| {
        shell
            .read(cx)
            .sends
            .get(PING)
            .map(|send| (send.tab, send.state.is_sending()))
    });
    assert_eq!(tab, Some((ResponseTab::Headers, true)));
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
}

#[gpui::test]
fn a_closed_port_fails_to_connect(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, closed_port());
    click(cx, "request-send");
    let text = settle(cx, &shell, PING).unwrap_or_default();
    assert!(
        text.starts_with("Failed: could not connect"),
        "unexpected {text}"
    );
}

#[gpui::test]
fn the_cancel_button_cancels_the_send(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    click(cx, "request-send");
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));
    assert!(cx.debug_bounds("request-send").is_none());

    click(cx, "request-cancel");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
    assert!(cx.debug_bounds("request-cancel").is_none());
    assert!(cx.debug_bounds("request-send").is_some());
}

#[gpui::test]
fn escape_cancels_the_send(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
}

#[gpui::test]
fn sending_again_replaces_the_send_in_flight(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    cx.simulate_keystrokes("ctrl-enter");
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    // The replaced send must not land later as a cancellation.
    thread::sleep(Duration::from_millis(50));
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
}

#[gpui::test]
fn closing_the_tab_drops_its_send(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    click(cx, "request-send");
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));

    click_owned(cx, &format!("doc-tab-close-{PING}"));
    assert!(cx.update(|_, cx| !shell.read(cx).sends.contains_key(PING)));
    thread::sleep(Duration::from_millis(50));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| !shell.read(cx).sends.contains_key(PING)));
}

/// Opens the environment menu and closes it with Escape.
fn open_and_escape_the_menu(cx: &mut VisualTestContext) {
    click(cx, "environment-selector");
    assert!(cx.debug_bounds("environment-menu").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("environment-menu").is_none());
}

#[gpui::test]
fn ctrl_enter_sends_after_the_environment_menu_closes(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    open_and_escape_the_menu(cx);
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
}

#[gpui::test]
fn escape_cancels_after_an_environment_is_chosen(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    click(cx, "environment-selector");
    click(cx, "environment-none");
    click(cx, "request-send");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
}

#[gpui::test]
fn sending_again_on_a_silent_server_stays_in_flight(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    cx.simulate_keystrokes("ctrl-enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-enter");
    thread::sleep(Duration::from_millis(100));
    cx.run_until_parked();
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));
}

#[gpui::test]
fn ctrl_enter_sends_after_the_menu_closes_in_the_app_root(cx: &mut TestAppContext) {
    let (shell, cx) = render_in(cx, serve(Server::Ok), true, &[]);
    open_and_escape_the_menu(cx);
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
}

#[gpui::test]
fn the_status_bar_shows_the_last_send(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    assert!(cx.debug_bounds("status-last-request").is_none());
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    let summary = cx.update(|_, cx| {
        let shell = shell.read(cx);
        shell.last_send.as_ref().and_then(|last| {
            let state = &shell.sends.get(&last.id)?.state;
            super::last_request::summary(&last.method, &last.path, state, Instant::now())
                .map(|(text, _)| text)
        })
    });
    assert!(
        summary
            .as_deref()
            .is_some_and(|text| text.starts_with("GET /ping · 200 · ")),
        "{summary:?}"
    );
    assert!(cx.debug_bounds("status-last-request").is_some());
}

#[gpui::test]
fn send_and_cancel_have_the_same_width(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    let send = match cx.debug_bounds("request-send") {
        Some(bounds) => bounds,
        None => panic!("request-send is missing"),
    };
    assert!(cx.debug_bounds("request-send-options").is_none());
    click(cx, "request-send");
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));
    let cancel = match cx.debug_bounds("request-cancel") {
        Some(bounds) => bounds,
        None => panic!("request-cancel is missing"),
    };
    assert_eq!(cancel.size, send.size);
    assert_eq!(cancel.right(), send.right());
}
