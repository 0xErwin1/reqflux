//! The Preview body view (response-preview S1-S3, S6) and the Cookies sub-tab (S4), against
//! a local HTTP server.

use std::cell::RefCell;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::Shell;
use super::request::{Completed, SendState};
use super::response::{BodyView, Preview, PreviewHost, cookie_count};
use super::send_tests::{PING, Server, click, render, serve, settle};

/// Starts a server that answers every connection with `200 OK`, `headers` and `body`.
pub(super) fn serve_reply(headers: &[(&str, &str)], body: Vec<u8>) -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("bind: {error}"),
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => panic!("address: {error}"),
    };
    let mut head = String::from("HTTP/1.1 200 OK\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    ));
    let mut reply = head.into_bytes();
    reply.extend_from_slice(&body);
    let reply = Arc::new(reply);
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let reply = Arc::clone(&reply);
            thread::spawn(move || {
                let mut request = Vec::new();
                let mut byte = [0_u8; 1];
                while !request.ends_with(b"\r\n\r\n")
                    && let Ok(1) = stream.read(&mut byte)
                {
                    request.extend_from_slice(&byte);
                }
                if let Err(error) = stream.write_all(&reply) {
                    eprintln!("write: {error}");
                }
            });
        }
    });
    port
}

/// A 2×1 PNG.
fn png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([200, 30, 30, 255]));
    if let Err(error) = image.write_to(&mut bytes, image::ImageFormat::Png) {
        panic!("encode: {error}");
    }
    bytes.into_inner()
}

const PAGE: &str = "<html><head><title>Hello &amp; welcome</title>\
    <script>document.write('hidden')</script><style>p{}</style></head>\
    <body><h1>Greeting</h1><p>One   &amp;\n two</p></body></html>";

fn present(cx: &mut VisualTestContext, selector: &'static str) {
    assert!(cx.debug_bounds(selector).is_some(), "{selector} is missing");
}

fn absent(cx: &mut VisualTestContext, selector: &'static str) {
    assert!(cx.debug_bounds(selector).is_none(), "{selector} is present");
}

/// Runs the executor until `done` holds, or fails after 10 s.
fn until(cx: &mut VisualTestContext, what: &str, done: impl Fn(&mut VisualTestContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "{what} never happened");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Sends `GET Ping` to `port` and waits for its response.
fn send(cx: &mut TestAppContext, port: u16) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = render(cx, port);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    (shell, cx)
}

/// Selects Preview and waits for it to be computed.
fn preview(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Arc<Preview> {
    click(cx, "response-view-preview");
    let read = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            shell
                .read(cx)
                .sends
                .get(PING)
                .and_then(|send| send.preview.clone())
        })
    };
    until(cx, "the preview", |cx| read(cx).is_some());
    match read(cx) {
        Some(preview) => preview,
        None => panic!("no preview"),
    }
}

#[gpui::test]
fn a_png_response_previews_with_its_size_and_format(cx: &mut TestAppContext) {
    let port = serve_reply(&[("content-type", "image/png")], png());
    let (shell, cx) = send(cx, port);
    present(cx, "response-view-preview");
    absent(cx, "response-preview");

    let preview = preview(cx, &shell);
    assert_eq!(preview.meta().as_deref(), Some("2×1 PNG"));
    present(cx, "response-preview-image");
    present(cx, "response-preview-meta");
    absent(cx, "response-body");
}

#[gpui::test]
fn an_image_by_magic_bytes_offers_preview(cx: &mut TestAppContext) {
    let port = serve_reply(&[("content-type", "application/octet-stream")], png());
    let (shell, cx) = send(cx, port);
    assert_eq!(preview(cx, &shell).meta().as_deref(), Some("2×1 PNG"));
}

#[gpui::test]
fn an_image_that_does_not_decode_says_so(cx: &mut TestAppContext) {
    let port = serve_reply(&[("content-type", "image/png")], b"not a png".to_vec());
    let (shell, cx) = send(cx, port);
    assert!(matches!(*preview(cx, &shell), Preview::Undecodable));
    present(cx, "response-preview-message");
    absent(cx, "response-preview-image");
}

/// Replaces the shell's preview host with one writing to `dir` and recording each open.
fn record_opens(
    cx: &mut VisualTestContext,
    shell: &Entity<Shell>,
    dir: PathBuf,
) -> Rc<RefCell<Vec<PathBuf>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&opened);
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            shell.preview_host = PreviewHost {
                dir: Some(dir),
                open: Rc::new(move |path, _| record.borrow_mut().push(path.to_owned())),
            };
        });
    });
    opened
}

#[gpui::test]
fn an_html_response_shows_its_title_and_text_and_opens_in_the_browser(cx: &mut TestAppContext) {
    let port = serve_reply(
        &[("content-type", "text/html; charset=utf-8")],
        PAGE.as_bytes().to_vec(),
    );
    let (shell, cx) = send(cx, port);
    let Preview::Html(page) = &*preview(cx, &shell) else {
        panic!("not an HTML preview");
    };
    assert_eq!(page.title.as_deref(), Some("Hello & welcome"));
    assert_eq!(page.paragraphs, ["Greeting", "One & two"]);
    present(cx, "response-preview-title");
    present(cx, "response-preview-text");
    present(cx, "response-preview-open");

    let cache = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("tempdir: {error}"),
    };
    let dir = cache.path().join("reqflux/preview");
    let opened = record_opens(cx, &shell, dir.clone());
    click(cx, "response-preview-open");
    until(cx, "the open", |_| !opened.borrow().is_empty());
    let path = opened.borrow().first().cloned().unwrap_or_default();
    assert_eq!(path.parent(), Some(dir.as_path()));
    assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("html"));
    assert_eq!(std::fs::read(&path).ok().as_deref(), Some(PAGE.as_bytes()));

    // A second open writes a new file.
    click(cx, "response-preview-open");
    until(cx, "the second open", |_| opened.borrow().len() == 2);
    assert_ne!(opened.borrow().get(1), Some(&path));
    absent(cx, "response-preview-error");
}

/// The readable text is a node of the tree the UI automation bridge serves, inside the pane,
/// with every paragraph in its label.
#[gpui::test]
fn the_html_text_reaches_the_automation_tree_inside_the_pane(cx: &mut TestAppContext) {
    let page = "<html><head><title>Preview Check</title><style>p{}</style></head>\
        <body><h1>Hello</h1><p>Fish &amp; chips</p><script>x()</script></body></html>";
    let port = serve_reply(&[("content-type", "text/html")], page.as_bytes().to_vec());
    let (shell, cx) = send(cx, port);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    preview(cx, &shell);
    cx.run_until_parked();

    let tree = automation.snapshot();
    let Some(pane) = tree.nodes.get("response-preview") else {
        panic!("response-preview is not in the tree");
    };
    let Some(text) = tree.nodes.get("response-preview-text") else {
        panic!(
            "response-preview-text is not in the tree: {:?}",
            tree.diagnostics
        );
    };
    let (Some(bounds), Some(outer)) = (text.bounds, pane.bounds) else {
        panic!("no bounds");
    };
    assert!(bounds.width > 0. && bounds.height > 0., "{bounds:?}");
    assert!(
        bounds.x >= outer.x
            && bounds.y >= outer.y
            && bounds.x + bounds.width <= outer.x + outer.width
            && bounds.y + bounds.height <= outer.y + outer.height,
        "{bounds:?} outside {outer:?}"
    );
    assert!(text.state.visible);
    assert_eq!(text.label.as_deref(), Some("Hello Fish & chips"));
    assert_eq!(
        text.text.as_ref().map(|text| text.text.as_str()),
        Some("Hello Fish & chips")
    );
}

/// A page whose text is longer than a label may be keeps its node, the label cut short.
#[gpui::test]
fn a_long_html_text_keeps_its_automation_node(cx: &mut TestAppContext) {
    let body = "<p>word word word word word word word</p>".repeat(400);
    let page = format!("<html><body>{body}</body></html>");
    let port = serve_reply(&[("content-type", "text/html")], page.into_bytes());
    let (shell, cx) = send(cx, port);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    preview(cx, &shell);
    cx.run_until_parked();

    let tree = automation.snapshot();
    let label = tree
        .nodes
        .get("response-preview-text")
        .and_then(|node| node.label.clone());
    let Some(label) = label else {
        panic!(
            "response-preview-text is not in the tree: {:?}",
            tree.diagnostics
        );
    };
    assert!(label.len() <= gpui_mcp::MAX_LABEL_BYTES, "{}", label.len());
    assert!(label.starts_with("word word"));
    assert!(label.ends_with('…'));
}

#[gpui::test]
fn a_preview_file_that_cannot_be_written_shows_the_error(cx: &mut TestAppContext) {
    let port = serve_reply(&[("content-type", "text/html")], PAGE.as_bytes().to_vec());
    let (shell, cx) = send(cx, port);
    preview(cx, &shell);
    let cache = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("tempdir: {error}"),
    };
    // A file where the directory should be.
    let blocked = cache.path().join("file");
    if let Err(error) = std::fs::write(&blocked, b"") {
        panic!("write: {error}");
    }
    let opened = record_opens(cx, &shell, blocked);
    click(cx, "response-preview-open");
    until(cx, "the error", |cx| {
        cx.debug_bounds("response-preview-error").is_some()
    });
    assert!(opened.borrow().is_empty());
}

#[gpui::test]
fn the_cookies_tab_lists_every_set_cookie(cx: &mut TestAppContext) {
    let port = serve_reply(
        &[
            ("content-type", "text/plain"),
            ("set-cookie", "sid=abc; Path=/; HttpOnly"),
            ("set-cookie", "theme=dark; Max-Age=60; Secure; SameSite=Lax"),
        ],
        b"ok".to_vec(),
    );
    let (shell, cx) = send(cx, port);
    absent(cx, "response-view-preview");
    let count = cx.update(
        |_, cx| match &shell.read(cx).sends.get(PING).map(|send| &send.state) {
            Some(SendState::Done(Completed::Response(received))) => cookie_count(&received.headers),
            _ => 0,
        },
    );
    assert_eq!(count, 2);

    click(cx, "response-tab-cookies");
    present(cx, "response-cookies");
    present(cx, "response-cookie-1");
    present(cx, "response-cookie-2");
    absent(cx, "response-cookie-3");
    absent(cx, "response-body");
}

#[gpui::test]
fn a_json_response_offers_no_preview_and_keeps_pretty_and_raw(cx: &mut TestAppContext) {
    let (shell, cx) = send(cx, serve(Server::Ok));
    absent(cx, "response-view-preview");
    present(cx, "response-view-pretty");
    present(cx, "response-view-raw");
    present(cx, "response-body-line-6");
    absent(cx, "response-body-line-7");

    click(cx, "response-view-raw");
    present(cx, "response-body-line-1");
    absent(cx, "response-body-line-2");

    // Preview selected on a body with nothing to preview shows Pretty.
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.select_body_view(PING, BodyView::Preview, cx);
        });
    });
    cx.run_until_parked();
    absent(cx, "response-preview");
    present(cx, "response-body-line-6");
}
