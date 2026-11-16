//! Send sends what the editors show, saved or not (feedback-nov16 S2): the bytes a local
//! server receives, not only the draft the window keeps.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use gpui::{TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::draft_tests::{bounds, click, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000A81";
const POSTED: &str = "01J00000000000000000000A82";

/// One request as the server read it: its request line and its body.
struct Received {
    line: String,
    /// The head, lower-cased, one `name: value` per line.
    head: String,
    body: String,
}

/// Reads one request off `stream`: the head, then `content-length` bytes of body.
fn read_request(stream: &mut TcpStream) -> Option<Received> {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match stream.read(&mut byte) {
            Ok(1) => head.extend_from_slice(&byte),
            Ok(_) | Err(_) => return None,
        }
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).ok()?;
    Some(Received {
        head: head.to_lowercase(),
        line: head.lines().next().unwrap_or_default().to_owned(),
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// Starts a server that answers `200 OK` and reports every request it reads.
fn capture() -> (u16, Receiver<Received>) {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("bind: {error}"),
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => panic!("address: {error}"),
    };
    let (sender, receiver) = channel();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let sender = sender.clone();
            thread::spawn(move || {
                if let Some(received) = read_request(&mut stream)
                    && sender.send(received).is_err()
                {
                    return;
                }
                let reply = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
                if let Err(error) = stream.write_all(reply.as_bytes()) {
                    eprintln!("write: {error}");
                }
            });
        }
    });
    (port, receiver)
}

/// A workspace holding `POST Posted` to `port`, with the raw JSON body `{"v":"saved"}`.
fn workspace(port: u16) -> TempDir {
    workspace_with(port, "", "")
}

/// [`workspace`] with `extra` appended to the request file and `collection` to the
/// collection file.
fn workspace_with(port: u16, extra: &str, collection: &str) -> TempDir {
    let dir = demo_workspace();
    write(
        dir.path(),
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n{collection}"),
    );
    write(
        dir.path(),
        "collections/api/posted.req.toml",
        &format!(
            "id = \"{POSTED}\"\nname = \"Posted\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:{port}/saved\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"v\":\"saved\"}}'\n{extra}"
        ),
    );
    dir
}

/// Waits for the next request the server reads.
fn next(cx: &mut VisualTestContext, receiver: &Receiver<Received>) -> Received {
    for _ in 0..100 {
        cx.run_until_parked();
        if let Ok(received) = receiver.recv_timeout(Duration::from_millis(50)) {
            return received;
        }
    }
    panic!("the server received no request");
}

fn replace_body(cx: &mut VisualTestContext, text: &str) {
    click(cx, "request-tab-body");
    // Coming back to the tab, the editor may still be open over the body.
    if bounds(cx, "request-body-input").is_some() {
        click(cx, "request-body-input");
    } else {
        click(cx, "request-body-code");
    }
    cx.simulate_keystrokes("ctrl-a backspace");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn send_sends_the_unsaved_body_after_a_saved_send(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    click(cx, "request-send");
    let first = next(cx, &receiver);
    assert_eq!(first.body, r#"{"v":"saved"}"#);

    replace_body(cx, "42");
    click(cx, "request-send");
    let second = next(cx, &receiver);
    assert_eq!(second.body, "42", "the unsaved body is sent");
    assert_eq!(second.line, "POST /saved HTTP/1.1");
}

#[gpui::test]
fn send_sends_the_unsaved_url_and_body_together(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    replace_body(cx, "7");
    click(cx, "request-url");
    cx.simulate_keystrokes("end backspace backspace backspace backspace backspace d r a f t");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-enter");
    let received = next(cx, &receiver);
    assert_eq!(received.line, "POST /draft HTTP/1.1");
    assert_eq!(received.body, "7");
}

const HEADERS: &str = "\n[[headers]]\nkey = \"X-One\"\nvalue = \"1\"\n";

/// Opens the value cell of the header row `n` and replaces its text, leaving the field open.
fn type_header(cx: &mut VisualTestContext, n: usize, text: &str) {
    click(cx, "request-tab-headers");
    click(cx, &format!("request-headers-value-{n}"));
    cx.simulate_keystrokes("ctrl-a");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn send_sends_a_header_whose_field_is_still_open(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(port, HEADERS, "");
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    type_header(cx, 1, "42");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(received.head.contains("x-one: 42\r\n"), "{}", received.head);
}

#[gpui::test]
fn send_sends_a_committed_header_and_then_the_body(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(port, HEADERS, "");
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    type_header(cx, 1, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    replace_body(cx, "7");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(received.head.contains("x-one: 42\r\n"), "{}", received.head);
    assert_eq!(received.body, "7");
}

#[gpui::test]
fn send_sends_the_body_after_a_header_after_the_body(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(port, HEADERS, "");
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    replace_body(cx, "7");
    type_header(cx, 1, "42");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    replace_body(cx, "8");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(received.head.contains("x-one: 42\r\n"), "{}", received.head);
    assert_eq!(received.body, "8");
}

/// Replaces the text of the Auth field `label`, leaving it open.
fn type_auth(cx: &mut VisualTestContext, label: &str, text: &str) {
    click(cx, "request-tab-auth");
    click(cx, &format!("request-auth-field-{label}"));
    cx.simulate_keystrokes("ctrl-a backspace");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn send_sends_a_bearer_token_whose_field_is_still_open(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(
        port,
        "\n[auth]\ntype = \"bearer\"\ntoken = \"stored\"\n",
        "",
    );
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    type_auth(cx, "token", "fresh");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(
        received.head.contains("authorization: bearer fresh\r\n"),
        "{}",
        received.head
    );
}

#[gpui::test]
fn send_sends_a_basic_username_whose_field_is_still_open(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(
        port,
        "\n[auth]\ntype = \"basic\"\nusername = \"old\"\npassword = \"pw\"\n",
        "",
    );
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    type_auth(cx, "username", "new");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    // base64("new:pw"), lower-cased with the head
    assert!(
        received.head.contains("authorization: basic bmv3onb3\r\n"),
        "{}",
        received.head
    );
}

#[gpui::test]
fn send_after_a_response_and_a_tab_switch_sends_the_edits(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = workspace_with(port, HEADERS, "");
    write(
        dir.path(),
        "collections/api/other.req.toml",
        "id = \"01J00000000000000000000A83\"\nname = \"Other\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:1/x\"\n",
    );
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));
    click(cx, "request-send");
    next(cx, &receiver);

    type_header(cx, 1, "42");
    click(cx, "tree-row-01J00000000000000000000A83");
    click(cx, &format!("tree-row-{POSTED}"));
    replace_body(cx, "9");
    type_header(cx, 1, "43");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(received.head.contains("x-one: 43\r\n"), "{}", received.head);
    assert_eq!(received.body, "9");
}

/// A workspace whose header `X-Var` is `{{who}}`, a collection variable valued `saved`.
fn variable_workspace(port: u16) -> TempDir {
    workspace_with(
        port,
        "\n[[headers]]\nkey = \"X-Var\"\nvalue = \"{{who}}\"\n",
        "\n[[variables]]\nkey = \"who\"\nvalue = \"saved\"\n",
    )
}

/// Opens the popover of the `{{who}}` mark in the header cell and replaces its value.
fn type_variable(cx: &mut VisualTestContext, text: &str) {
    click(cx, "request-tab-headers");
    click(cx, "request-headers-value-1-var-1");
    cx.simulate_keystrokes("ctrl-a");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn send_sends_a_variable_edited_in_the_popover_and_confirmed(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = variable_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    type_variable(cx, "fresh");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(
        received.head.contains("x-var: fresh\r\n"),
        "{}",
        received.head
    );
}

#[gpui::test]
fn send_after_a_popover_value_left_unconfirmed_sends_the_stored_variable(cx: &mut TestAppContext) {
    let (port, receiver) = capture();
    let dir = variable_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{POSTED}"));

    // ARCHITECTURE.md ("variable popover"): Enter writes, a click outside closes the popover
    // without writing, so the Send click discards the value typed in it. Pinned as the
    // documented behavior; see the parent report for the open question.
    type_variable(cx, "fresh");
    click(cx, "request-send");
    let received = next(cx, &receiver);
    assert!(
        received.head.contains("x-var: saved\r\n"),
        "{}",
        received.head
    );
}
