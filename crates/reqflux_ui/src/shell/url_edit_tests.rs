//! The URL's query and the Params grid are one thing (S7 of request-pane-feedback), and
//! Enter in the URL field commits and sends (S8).

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{KeyValue, Request};
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, edit_url, present, render, state_file};
use crate::shell::send_tests::{Server, closed_port, response, serve};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J000000000000000000000Q1";
const QUERY: &str = "01J000000000000000000000Q2";

/// A workspace holding `GET Query` at `http://127.0.0.1:<port>/q` with the params `a=1`,
/// the disabled `off=x` and `b={{v}}`.
fn query_workspace(port: u16) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        "collections/api/query.req.toml",
        &format!(
            "id = \"{QUERY}\"\nname = \"Query\"\nmethod = \"GET\"\nurl = \"http://127.0.0.1:{port}/q\"\n\n\
             [[params]]\nkey = \"a\"\nvalue = \"1\"\n\n\
             [[params]]\nkey = \"off\"\nvalue = \"x\"\nenabled = false\n\n\
             [[params]]\nkey = \"b\"\nvalue = \"{{{{v}}}}\"\n"
        ),
    );
    dir
}

fn draft(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Request> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(QUERY)
            .map(|draft| draft.request.clone())
    })
}

fn shown(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> String {
    cx.update(|_, cx| shell.read(cx).shown_url(QUERY))
}

fn field_text(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<String> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .url_edit
            .as_ref()
            .map(|edit| edit.input.read(cx).value().to_string())
    })
}

fn off() -> KeyValue {
    KeyValue {
        enabled: false,
        ..KeyValue::new("off", "x")
    }
}

#[gpui::test]
fn the_line_and_the_field_show_the_enabled_params_as_written(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = query_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{QUERY}"));
    let url = format!("http://127.0.0.1:{port}/q?a=1&b={{{{v}}}}");
    assert_eq!(shown(cx, &shell), url);

    click(cx, "request-url");
    assert_eq!(field_text(cx, &shell).as_deref(), Some(url.as_str()));
}

#[gpui::test]
fn a_typed_query_becomes_rows_and_keeps_the_disabled_ones(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = query_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{QUERY}"));

    let base = format!("http://127.0.0.1:{port}/q");
    edit_url(cx, &format!("{base}?test=test&{{{{k}}}}=2&test=3"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let Some(request) = draft(cx, &shell) else {
        panic!("typing makes a draft");
    };
    assert_eq!(request.url, base, "the stored URL is the base");
    assert_eq!(
        request.params,
        vec![
            KeyValue::new("test", "test"),
            off(),
            KeyValue::new("{{k}}", "2"),
            KeyValue::new("test", "3"),
        ]
    );
    assert_eq!(
        shown(cx, &shell),
        format!("{base}?test=test&{{{{k}}}}=2&test=3")
    );
    click(cx, "request-tab-params");
    present(cx, "request-params-key-4");
    assert!(bounds(cx, "request-params-key-5").is_none());
}

#[gpui::test]
fn toggling_and_deleting_rows_updates_the_shown_url(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = query_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{QUERY}"));
    click(cx, "request-tab-params");
    let base = format!("http://127.0.0.1:{port}/q");

    click(cx, "request-params-enabled-1");
    assert_eq!(shown(cx, &shell), format!("{base}?b={{{{v}}}}"));
    click(cx, "request-params-enabled-2");
    assert_eq!(shown(cx, &shell), format!("{base}?off=x&b={{{{v}}}}"));
    click(cx, "request-params-delete-3");
    assert_eq!(shown(cx, &shell), format!("{base}?off=x"));
}

#[gpui::test]
fn escape_restores_the_url_and_the_rows(cx: &mut TestAppContext) {
    let port = closed_port();
    let dir = query_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{QUERY}"));

    edit_url(cx, "http://h/other?z=9");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(draft(cx, &shell).is_none(), "no draft is left");
    assert_eq!(
        shown(cx, &shell),
        format!("http://127.0.0.1:{port}/q?a=1&b={{{{v}}}}")
    );
}

#[gpui::test]
fn enter_in_the_url_field_commits_and_sends(cx: &mut TestAppContext) {
    let port = serve(Server::Silent);
    let dir = query_workspace(port);
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    click(cx, &format!("tree-row-{QUERY}"));

    edit_url(cx, &format!("http://127.0.0.1:{port}/q?only=1"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(response(cx, &shell, QUERY).as_deref(), Some("Sending"));
    assert!(field_text(cx, &shell).is_none(), "the field closed");
    let params = draft(cx, &shell).map(|request| request.params);
    assert_eq!(params, Some(vec![KeyValue::new("only", "1"), off()]));
}
