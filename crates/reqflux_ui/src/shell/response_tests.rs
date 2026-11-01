//! The response view: the tab row (S5), the body and headers (S6), the error banner and
//! Retry (S7) and the empty state, against a local HTTP server; and the body search, the
//! Tests and Console filters, the cookie table and Copy (app-pending S7).

use gpui::{Pixels, TestAppContext, VisualTestContext};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use super::draft_tests::state_file;
use super::preview_tests::serve_reply;
use super::send_tests::{
    JSON, PING, Server, click, closed_port, render, render_files, response, serve, settle,
};
use crate::workspace_tests::{demo_workspace, write};

const PASSING_TEST: &str =
    "pm.test(\"status is 200\", function () {\n  pm.expect(pm.response.code).to.eql(200);\n});\n";
const FAILING_TEST: &str =
    "pm.test(\"wrong code\", function () {\n  pm.expect(200, \"bad code\").to.eql(404);\n});\n";
/// Records a test only when the response is a 200, so a later response has no results.
const CONDITIONAL_TEST: &str =
    "if (pm.response.code === 200) {\n  pm.test(\"ok\", function () {});\n}\n";
/// Logs only when the response is a 200, so a later response has no console entries. The
/// guard runs in the test stage, the only one with `pm.response`.
const CONDITIONAL_CONSOLE: &str =
    "if (pm.response.code === 200) {\n  console.log(\"logged\");\n}\n";

/// A port whose first connection answers `200 OK` and every later one with
/// `503 Service Unavailable`, so a script can produce results for the first response only.
fn flaky_port() -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => panic!("bind: {error}"),
    };
    let port = match listener.local_addr() {
        Ok(address) => address.port(),
        Err(error) => panic!("address: {error}"),
    };
    thread::spawn(move || {
        for (n, mut stream) in listener.incoming().flatten().enumerate() {
            thread::spawn(move || {
                let mut head = Vec::new();
                let mut byte = [0_u8; 1];
                while !head.ends_with(b"\r\n\r\n")
                    && let Ok(1) = stream.read(&mut byte)
                {
                    head.extend_from_slice(&byte);
                }
                let code = if n == 0 {
                    "200 OK"
                } else {
                    "503 Service Unavailable"
                };
                let reply = format!(
                    "HTTP/1.1 {code}\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}"
                );
                if let Err(error) = stream.write_all(reply.as_bytes()) {
                    eprintln!("write: {error}");
                }
            });
        }
    });
    port
}

fn present(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<Pixels> {
    match cx.debug_bounds(Box::leak(selector.to_owned().into_boxed_str())) {
        Some(bounds) => bounds,
        None => panic!("{selector} is missing"),
    }
}

fn absent(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_none()
}

/// The tops of the body lines `1..=count`, which must all be present.
fn line_tops(cx: &mut VisualTestContext, count: usize) -> Vec<Pixels> {
    (1..=count)
        .map(|n| present(cx, &format!("response-body-line-{n}")).origin.y)
        .collect()
}

#[gpui::test]
fn a_request_never_sent_shows_no_response(cx: &mut TestAppContext) {
    let (_, cx) = render(cx, serve(Server::Ok));
    present(cx, "request-pane");
    for id in [
        "response",
        "response-empty",
        "response-status",
        "response-body",
    ] {
        assert!(absent(cx, id), "{id} shows before a send");
    }
}

#[gpui::test]
fn a_json_response_shows_its_status_meta_and_pretty_body(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    present(cx, "response-status");
    present(cx, "response-meta");
    present(cx, "response-tab-body");
    present(cx, "response-view-pretty");
    assert!(absent(cx, "response-empty"));
    let body = present(cx, "response-body");
    let tops = line_tops(cx, 6);
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
    assert!(tops.iter().all(|top| *top >= body.origin.y));
    assert!(absent(cx, "response-body-line-7"));

    click(cx, "response-view-raw");
    line_tops(cx, 1);
    assert!(absent(cx, "response-body-line-2"));
}

#[gpui::test]
fn the_headers_tab_lists_the_response_headers(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    settle(cx, &shell, PING);

    click(cx, "response-tab-headers");
    for n in 1..=3 {
        present(cx, &format!("response-headers-row-{n}"));
    }
    assert!(absent(cx, "response-headers-row-4"));
    assert!(absent(cx, "response-body"));
}

#[gpui::test]
fn a_connection_error_shows_the_banner(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, closed_port());
    click(cx, "request-send");
    settle(cx, &shell, PING);

    present(cx, "response-status");
    present(cx, "response-meta");
    present(cx, "response-error");
    present(cx, "response-retry");
    assert!(absent(cx, "response-body"));
}

#[gpui::test]
fn retry_sends_again(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Silent));
    click(cx, "request-send");
    present(cx, "response-progress");
    present(cx, "response-elapsed");
    click(cx, "request-cancel");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("Failed: request cancelled")
    );
    assert!(absent(cx, "response-progress"));

    click(cx, "response-retry");
    assert_eq!(response(cx, &shell, PING).as_deref(), Some("Sending"));
    // The failed send stays visible, dimmed, under the progress bar.
    present(cx, "response-progress");
    present(cx, "response-error");
}

#[gpui::test]
fn a_passing_test_script_shows_the_tests_tab(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", PASSING_TEST)]);
    assert!(absent(cx, "response-tab-tests"));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    present(cx, "response-tab-tests");
    assert!(absent(cx, "response-tab-console"));
    click(cx, "response-tab-tests");
    present(cx, "response-test-1");
    assert!(absent(cx, "response-test-1-message"));
    assert!(absent(cx, "response-test-2"));
}

#[gpui::test]
fn send_runs_the_drafts_unsaved_test_script(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", FAILING_TEST)]);
    shell.update(cx, |shell, cx| {
        shell.edit_draft_scripts(
            PING,
            |scripts| scripts.test = Some(PASSING_TEST.to_owned()),
            cx,
        );
    });
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    click(cx, "response-tab-tests");
    present(cx, "response-test-1");
    assert!(
        absent(cx, "response-test-1-message"),
        "the unsaved passing test ran, not the saved failing one"
    );
    assert!(absent(cx, "response-test-2"));
}

#[gpui::test]
fn a_failing_test_shows_its_message(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", FAILING_TEST)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    click(cx, "response-tab-tests");
    present(cx, "response-test-1");
    present(cx, "response-test-1-message");
}

#[gpui::test]
fn console_entries_show_their_level_and_message(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(
        cx,
        serve(Server::Ok),
        &[
            ("ping.pre.js", "console.warn(\"from the pre-request\");\n"),
            (
                "ping.test.js",
                "console.log(\"from the test\");\npm.test(\"ok\", function () {});\n",
            ),
        ],
    );
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    present(cx, "response-tab-console");
    present(cx, "response-tab-tests");
    click(cx, "response-tab-console");
    present(cx, "response-console-1");
    present(cx, "response-console-2");
    assert!(absent(cx, "response-console-3"));
}

#[gpui::test]
fn a_failing_send_still_shows_its_pre_request_console(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(
        cx,
        closed_port(),
        &[(
            "ping.pre.js",
            "console.log(\"from the failed pre-request\");\n",
        )],
    );
    click(cx, "request-send");
    assert!(
        settle(cx, &shell, PING)
            .unwrap_or_default()
            .starts_with("Failed: ")
    );

    // The pre-request ran before the transport failed; its console row shows under the
    // error banner.
    present(cx, "response-error");
    present(cx, "response-error-console-1");
    assert!(absent(cx, "response-error-test-1"));
}

#[gpui::test]
fn a_response_without_scripts_shows_neither_entry(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    assert!(absent(cx, "response-tab-tests"));
    assert!(absent(cx, "response-tab-console"));
    present(cx, "response-tab-headers");
}

#[gpui::test]
fn tests_selected_then_a_response_without_tests_falls_back_to_body(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, flaky_port(), &[("ping.test.js", CONDITIONAL_TEST)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "response-tab-tests");
    present(cx, "response-test-1");

    click(cx, "request-send");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("503 Service Unavailable")
    );
    // The Tests chip is gone with its results; the pane shows the Body again.
    assert!(absent(cx, "response-tab-tests"));
    assert!(absent(cx, "response-tests"));
    present(cx, "response-body");
}

#[gpui::test]
fn console_selected_then_a_response_without_entries_falls_back_to_body(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, flaky_port(), &[("ping.test.js", CONDITIONAL_CONSOLE)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "response-tab-console");
    present(cx, "response-console-1");

    click(cx, "request-send");
    assert_eq!(
        settle(cx, &shell, PING).as_deref(),
        Some("503 Service Unavailable")
    );
    assert!(absent(cx, "response-tab-console"));
    assert!(absent(cx, "response-console"));
    present(cx, "response-body");
}

#[gpui::test]
fn tests_and_console_sit_in_the_tab_row_beside_response(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(
        cx,
        serve(Server::Ok),
        &[(
            "ping.test.js",
            "console.log(\"from the test\");\npm.test(\"ok\", function () {});\n",
        )],
    );
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    let response = present(cx, "response-tab-response");
    let tests = present(cx, "response-tab-tests");
    let console = present(cx, "response-tab-console");
    let views = present(cx, "response-views");
    for tab in [tests, console] {
        assert_eq!(tab.center().y, response.center().y);
        assert!(tab.origin.x > response.origin.x);
        assert!(tab.bottom() <= views.origin.y);
    }

    // A script tab replaces the HTTP views and their bottom bar.
    click(cx, "response-tab-tests");
    present(cx, "response-test-1");
    assert!(absent(cx, "response-views"));
    assert!(absent(cx, "response-footer"));
}

#[gpui::test]
fn the_body_format_switch_sits_in_the_bottom_bar(cx: &mut TestAppContext) {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    let views = present(cx, "response-views");
    let body_tab = present(cx, "response-tab-body");
    let body = present(cx, "response-body");
    let footer = present(cx, "response-footer");
    assert!(views.contains(&body_tab.center()));
    assert!(footer.origin.y >= body.bottom());
    for view in [
        "response-view-pretty",
        "response-view-raw",
        "response-footer-lines",
    ] {
        let bounds = present(cx, view);
        assert!(footer.contains(&bounds.center()), "{view} outside the bar");
    }

    // Headers has no format switch.
    click(cx, "response-tab-headers");
    assert!(absent(cx, "response-footer"));
    assert!(absent(cx, "response-view-pretty"));
}

/// The accessible label of the element `selector`, which must be on screen.
fn label(cx: &mut VisualTestContext, selector: &str) -> String {
    present(cx, selector);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .get(selector)
        .and_then(|node| node.label.clone())
        .unwrap_or_default()
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

/// Sends `GET Ping`, whose JSON body pretty-prints as six lines, two holding an `a`
/// (`"tags"` and `"a"`).
fn sent(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (shell, cx) = render(cx, serve(Server::Ok));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    cx
}

#[gpui::test]
fn ctrl_f_in_the_response_searches_the_body(cx: &mut TestAppContext) {
    let cx = sent(cx);
    present(cx, "response-search-keycap");
    assert!(absent(cx, "response-search-count"));

    click(cx, "response-body");
    press(cx, "ctrl-f");
    assert_eq!(label(cx, "response-search-count"), "0 / 0");
    type_text(cx, "A");
    assert_eq!(label(cx, "response-search-count"), "1 / 2");
    present(cx, "response-body-hit-1");
    present(cx, "response-body-hit-2");
    assert!(absent(cx, "response-body-hit-3"));

    press(cx, "enter");
    assert_eq!(label(cx, "response-search-count"), "2 / 2");
    press(cx, "enter");
    assert_eq!(label(cx, "response-search-count"), "1 / 2", "wraps around");
    press(cx, "shift-enter");
    assert_eq!(label(cx, "response-search-count"), "2 / 2");
    click(cx, "response-search-next");
    assert_eq!(label(cx, "response-search-count"), "1 / 2");
    click(cx, "response-search-previous");
    assert_eq!(label(cx, "response-search-count"), "2 / 2");

    type_text(cx, "zz");
    assert_eq!(label(cx, "response-search-count"), "0 / 0");
    assert!(absent(cx, "response-body-hit-1"));
}

#[gpui::test]
fn escape_closes_the_search_and_its_highlights(cx: &mut TestAppContext) {
    let cx = sent(cx);
    click(cx, "response-search");
    present(cx, "response-search-count");
    type_text(cx, "tags");
    present(cx, "response-body-hit-1");

    press(cx, "escape");
    assert!(absent(cx, "response-search-count"));
    assert!(absent(cx, "response-body-hit-1"));
    present(cx, "response-search-keycap");
    present(cx, "response-body-line-1");
}

#[gpui::test]
fn ctrl_f_outside_the_response_does_not_search(cx: &mut TestAppContext) {
    let cx = sent(cx);
    click(cx, "request-pane");
    press(cx, "ctrl-f");
    assert!(absent(cx, "response-search-count"));
}

#[gpui::test]
fn copy_puts_the_body_on_the_clipboard(cx: &mut TestAppContext) {
    let cx = sent(cx);
    let copy = present(cx, "response-copy");
    assert!(present(cx, "response-footer").contains(&copy.center()));
    click(cx, "response-copy");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some(JSON));
}

/// A test script with a passing and a failing test, and console entries at every level.
const MIXED_RESULTS: &str = "pm.test(\"passes\", function () {});\n\
    pm.test(\"fails\", function () {\n  pm.expect(1, \"bad\").to.eql(2);\n});\n\
    console.log(\"plain\");\nconsole.warn(\"careful\");\nconsole.error(\"broken\");\n";

#[gpui::test]
fn the_tests_filter_keeps_the_failed_tests(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", MIXED_RESULTS)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "response-tab-tests");
    assert_eq!(label(cx, "response-tests-passed"), "1 passed");
    assert_eq!(label(cx, "response-tests-failed"), "1 failed");
    present(cx, "response-test-1");
    present(cx, "response-test-2");

    click(cx, "response-tests-filter-failed");
    assert!(absent(cx, "response-test-1"));
    present(cx, "response-test-2");
    present(cx, "response-test-2-message");

    click(cx, "response-tests-filter-all");
    present(cx, "response-test-1");
    present(cx, "response-test-2");
}

#[gpui::test]
fn failed_without_failures_says_so(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", PASSING_TEST)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "response-tab-tests");
    click(cx, "response-tests-filter-failed");
    assert!(absent(cx, "response-test-1"));
    assert_eq!(label(cx, "response-tests-none"), "No failed tests");
}

#[gpui::test]
fn the_console_filter_keeps_warnings_or_errors(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", MIXED_RESULTS)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));
    click(cx, "response-tab-console");
    present(cx, "response-console-note");
    for n in 1..=3 {
        present(cx, &format!("response-console-{n}"));
    }

    click(cx, "response-console-filter-warnings");
    assert_eq!(label(cx, "response-console-2"), "warn: careful");
    assert!(absent(cx, "response-console-1"));
    assert!(absent(cx, "response-console-3"));

    click(cx, "response-console-filter-errors");
    assert_eq!(label(cx, "response-console-3"), "error: broken");
    assert!(absent(cx, "response-console-1"));
    assert!(absent(cx, "response-console-2"));

    click(cx, "response-console-filter-all");
    present(cx, "response-console-1");
}

const KEYED: &str = "01J00000000000000000000036";
const TOKEN: &str = "tok-s3cret-value";

#[gpui::test]
fn the_cookie_table_has_its_columns_and_hides_a_secret_value(cx: &mut TestAppContext) {
    let port = serve_reply(
        &[
            ("content-type", "text/plain"),
            (
                "set-cookie",
                &format!("sid={TOKEN}; Path=/; HttpOnly; Secure"),
            ),
            (
                "set-cookie",
                "region=eu; Domain=.acme.com; Path=/; Expires=Mon, 30 Nov 2026 10:05:00 GMT; SameSite=Lax",
            ),
        ],
        b"ok".to_vec(),
    );
    let dir = demo_workspace();
    write(
        dir.path(),
        "collections/api/collection.toml",
        "id = \"01J00000000000000000000035\"\nname = \"API\"\n",
    );
    write(
        dir.path(),
        "collections/api/keyed.req.toml",
        &format!(
            "id = \"{KEYED}\"\nname = \"Keyed\"\nmethod = \"GET\"\n\
             url = \"http://127.0.0.1:{port}/\"\n\n[auth]\ntype = \"bearer\"\ntoken = \"{TOKEN}\"\n"
        ),
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = super::draft_tests::render(cx, &dir, &state_path);
    click_selector(cx, &format!("tree-row-{KEYED}"));
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, KEYED).as_deref(), Some("200 OK"));

    click(cx, "response-tab-cookies");
    present(cx, "response-cookies-header");
    present(cx, "response-cookie-1-secret");
    let secret_row = label(cx, "response-cookie-1");
    assert!(!secret_row.contains(TOKEN), "{secret_row}");
    assert!(secret_row.starts_with("sid=••"), "{secret_row}");
    assert_eq!(
        label(cx, "response-cookie-2"),
        "region=eu · .acme.com · / · 30 Nov 2026, 10:05 UTC · SameSite=Lax"
    );
    assert!(absent(cx, "response-cookie-2-secret"));
    for column in ["name", "value", "domain", "path", "expires", "flags"] {
        present(cx, &format!("response-cookie-2-{column}"));
    }
}

fn click_selector(cx: &mut VisualTestContext, selector: &str) {
    click(cx, Box::leak(selector.to_owned().into_boxed_str()));
}

#[gpui::test]
fn the_response_tab_returns_from_a_script_tab_to_the_body(cx: &mut TestAppContext) {
    let (shell, cx) = render_files(cx, serve(Server::Ok), &[("ping.test.js", PASSING_TEST)]);
    click(cx, "request-send");
    assert_eq!(settle(cx, &shell, PING).as_deref(), Some("200 OK"));

    click(cx, "response-tab-tests");
    assert!(absent(cx, "response-body"));
    click(cx, "response-tab-response");
    present(cx, "response-body");
    present(cx, "response-view-pretty");
    assert!(absent(cx, "response-tests"));
}
