//! Send sends what the editors show, saved or not (feedback-nov16 S2): the bytes a local
//! server receives, not only the draft the window keeps.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use gpui::{TestAppContext, VisualTestContext};
use tempfile::TempDir;

use super::draft_tests::{click, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000A81";
const POSTED: &str = "01J00000000000000000000A82";

/// One request as the server read it: its request line and its body.
struct Received {
    line: String,
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
    let dir = demo_workspace();
    write(
        dir.path(),
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        dir.path(),
        "collections/api/posted.req.toml",
        &format!(
            "id = \"{POSTED}\"\nname = \"Posted\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:{port}/saved\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = '{{\"v\":\"saved\"}}'\n"
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
    click(cx, "request-body-code");
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
