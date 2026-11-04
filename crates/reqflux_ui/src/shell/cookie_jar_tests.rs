//! Workspace > Cookies (v1-features S5, design/network/01-settings-sections): the jar table
//! with its filter, a delete per row and Clear all behind a confirmation, and the rule that a
//! cookie shows its value unless it is a secret. The model tests need no window; the section's
//! are driven over a temp workspace and state file.

use std::path::Path;

use gpui::{TestAppContext, VisualTestContext};
use reqflux_engine::{MASK, StoredCookie};
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::shell::send_tests::closed_port;
use crate::shell::settings_page::cookie_jar::{
    self, CookieValue, JarRow, count_text, expires_text,
};
use crate::shell::variables_tests::variables_workspace;

fn cookie(domain: &str, name: &str, value: &str, expires: Option<i64>) -> StoredCookie {
    StoredCookie {
        domain: domain.to_owned(),
        path: "/".to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
        expires,
        secure: false,
        http_only: false,
        host_only: true,
        same_site: None,
    }
}

fn shown(value: &str) -> CookieValue {
    CookieValue::Shown(value.to_owned())
}

#[test]
fn a_value_that_holds_a_secret_is_never_shown() {
    assert_eq!(
        cookie_jar::cookie_value("plain", str::to_owned),
        shown("plain")
    );
    assert_eq!(
        cookie_jar::cookie_value("s3cret", |_| MASK.to_owned()),
        CookieValue::Secret
    );
    // Already masked, as a recorded header leaves it.
    assert_eq!(
        cookie_jar::cookie_value(MASK, str::to_owned),
        CookieValue::Secret
    );
}

#[test]
fn the_filter_matches_domain_and_name_and_the_place_is_kept() {
    let jar = vec![
        cookie("api.example.com", "session", "a", None),
        cookie("cdn.example.net", "asset", "b", None),
    ];
    let all = cookie_jar::rows(&jar, "", str::to_owned, false);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].n, 2);
    let by_name = cookie_jar::rows(&jar, "  SESSION ", str::to_owned, false);
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].name, "session");
    assert_eq!(by_name[0].n, 1, "the place is the one in the whole jar");
    let by_domain = cookie_jar::rows(&jar, "example.net", str::to_owned, false);
    assert_eq!(by_domain.len(), 1);
    assert_eq!(by_domain[0].domain, "cdn.example.net");
    assert!(cookie_jar::rows(&jar, "nothing", str::to_owned, false).is_empty());
}

#[test]
fn hide_all_hides_every_value() {
    let jar = vec![cookie("a.test", "one", "x", None)];
    let rows: Vec<JarRow> = cookie_jar::rows(&jar, "", str::to_owned, true);
    assert_eq!(rows[0].value, CookieValue::Secret);
}

#[test]
fn the_flags_and_the_expiry_read_as_the_table_shows_them() {
    let mut entry = cookie("a.test", "one", "x", Some(1_800_000_000));
    entry.secure = true;
    entry.http_only = true;
    entry.same_site = Some("Lax".to_owned());
    let rows = cookie_jar::rows(&[entry], "", str::to_owned, false);
    assert_eq!(rows[0].flags, vec!["Secure", "HttpOnly", "SameSite=Lax"]);
    assert_eq!(rows[0].expires, "15 Jan 2027, 08:00 UTC");
    assert_eq!(expires_text(None), "Session");
}

#[test]
fn the_count_says_domains_or_the_filtered_share() {
    let jar = vec![
        cookie("a.test", "one", "x", None),
        cookie("a.test", "two", "y", None),
        cookie("b.test", "three", "z", None),
    ];
    assert_eq!(count_text(&jar, 3, false), "3 cookies · 2 domains");
    assert_eq!(count_text(&jar, 1, true), "1 of 3");
}

/// Renders the shell over a temp workspace and state file, and opens the jar.
fn render_jar<'a>(
    cx: &'a mut TestAppContext,
    dir: &TempDir,
    state_path: &Path,
) -> (gpui::Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = render(cx, dir, state_path);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.open_cookie_jar(window, cx)));
    cx.run_until_parked();
    (shell, cx)
}

#[gpui::test]
fn the_jar_lists_the_cookies_of_the_workspace(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);
    cx.update(|_window, cx| {
        shell.update(cx, |shell, cx| {
            let crate::workspace::OpenedWorkspace::Loaded(engine) = &shell.workspace else {
                panic!("no workspace");
            };
            if let Err(error) = engine.add_cookie(
                "https://api.example.com/",
                "session=abc123; Path=/; Secure; HttpOnly",
            ) {
                panic!("add cookie: {error}");
            }
            cx.notify();
        });
    });
    cx.run_until_parked();
    present(cx, "cookies-table");
    assert_eq!(shell.read_with(cx, |shell, cx| shell.jar_rows(cx).len()), 1);
}

#[gpui::test]
fn clear_all_asks_first_and_empties_the_jar(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);
    cx.update(|_window, cx| {
        shell.update(cx, |shell, cx| {
            let crate::workspace::OpenedWorkspace::Loaded(engine) = &shell.workspace else {
                panic!("no workspace");
            };
            if let Err(error) = engine.add_cookie(
                "https://api.example.com/",
                "session=abc123; Path=/; Secure; HttpOnly",
            ) {
                panic!("add cookie: {error}");
            }
            cx.notify();
        });
    });
    cx.run_until_parked();
    click(cx, "cookies-clear");
    present(cx, "cookies-clear-dialog");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.jar_cookies().len()),
        1,
        "asking deletes nothing"
    );
    click(cx, "cookies-clear-cancel");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.jar_cookies().len()),
        1,
        "cancel deletes nothing"
    );
    click(cx, "cookies-clear");
    present(cx, "cookies-clear-dialog");
    click(cx, "cookies-clear-confirm");
    assert!(shell.read_with(cx, |shell, _| shell.jar_cookies().is_empty()));
    present(cx, "cookies-empty");
}

fn fields() -> cookie_jar::edit::Fields {
    cookie_jar::edit::Fields {
        name: " theme ".to_owned(),
        value: "dark".to_owned(),
        domain: "example.test".to_owned(),
        path: "/".to_owned(),
        session: true,
        expires: String::new(),
        secure: true,
        http_only: false,
        subdomains: false,
        same_site: "Lax".to_owned(),
    }
}

#[test]
fn the_fields_become_a_cookie_with_a_session_or_a_utc_date() {
    let session = cookie_jar::edit::cookie_from(&fields()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(session.name, "theme");
    assert!(session.host_only && session.secure && !session.http_only);
    assert_eq!(session.expires, None);
    assert_eq!(session.same_site.as_deref(), Some("Lax"));

    let dated = cookie_jar::edit::Fields {
        session: false,
        expires: "2027-01-15 08:00".to_owned(),
        subdomains: true,
        same_site: String::new(),
        ..fields()
    };
    let dated = cookie_jar::edit::cookie_from(&dated).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(dated.expires, Some(1_800_000_000));
    assert!(!dated.host_only);
    assert_eq!(dated.same_site, None);
    assert_eq!(
        cookie_jar::edit::expires_input(Some(1_800_000_000)),
        "2027-01-15 08:00"
    );
    assert_eq!(cookie_jar::edit::expires_input(None), "");
}

#[test]
fn a_date_that_does_not_parse_says_how_to_write_it() {
    let bad = cookie_jar::edit::Fields {
        session: false,
        expires: "tomorrow".to_owned(),
        ..fields()
    };
    assert_eq!(
        cookie_jar::edit::cookie_from(&bad),
        Err("Expires must be a UTC date such as 2026-12-31 10:00")
    );
    assert_eq!(
        cookie_jar::edit::parse_expires("2027-01-15"),
        Ok(1_799_971_200)
    );
}

fn type_into(cx: &mut VisualTestContext, field: &str, text: &str) {
    click(cx, field);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn jar(shell: &gpui::Entity<Shell>, cx: &mut VisualTestContext) -> Vec<StoredCookie> {
    shell.read_with(cx, |shell, _| shell.jar_cookies())
}

#[gpui::test]
fn add_cookie_writes_the_fields_to_the_jar(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);

    click(cx, "cookies-add");
    present(cx, "cookie-edit-dialog");
    type_into(cx, "cookie-edit-name", "theme");
    type_into(cx, "cookie-edit-value", "dark");
    type_into(cx, "cookie-edit-domain", "example.test");
    click(cx, "cookie-edit-secure");
    click(cx, "cookie-edit-save");

    let cookies = jar(&shell, cx);
    assert_eq!(cookies.len(), 1);
    let saved = &cookies[0];
    assert_eq!(
        (
            saved.name.as_str(),
            saved.value.as_str(),
            saved.domain.as_str()
        ),
        ("theme", "dark", "example.test")
    );
    assert_eq!(saved.path, "/");
    assert!(saved.secure && saved.expires.is_none());
    assert!(
        bounds(cx, "cookie-edit-dialog").is_none(),
        "the dialog closes"
    );
}

#[gpui::test]
fn a_refused_cookie_shows_the_engine_text_and_keeps_the_dialog(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);

    click(cx, "cookies-add");
    type_into(cx, "cookie-edit-value", "secret-1");
    click(cx, "cookie-edit-save");

    present(cx, "cookie-edit-error");
    present(cx, "cookie-edit-dialog");
    assert!(jar(&shell, cx).is_empty());
    click(cx, "cookie-edit-cancel");
    assert!(bounds(cx, "cookie-edit-dialog").is_none());
    assert!(jar(&shell, cx).is_empty(), "cancel writes nothing");
}

#[gpui::test]
fn the_pencil_edits_a_cookie_and_a_new_name_moves_it(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);
    shell.update(cx, |shell, cx| {
        let crate::workspace::OpenedWorkspace::Loaded(engine) = &shell.workspace else {
            panic!("no workspace");
        };
        engine
            .set_cookie(&cookie("example.test", "old", "v1", None))
            .unwrap_or_else(|e| panic!("{e}"));
        cx.notify();
    });
    cx.run_until_parked();

    click(cx, "cookie-1-edit");
    present(cx, "cookie-edit-dialog");
    type_into(cx, "cookie-edit-name", "renamed");
    type_into(cx, "cookie-edit-value", "v2");
    click(cx, "cookie-edit-save");

    let cookies = jar(&shell, cx);
    assert_eq!(cookies.len(), 1, "the old name is gone");
    assert_eq!(
        (cookies[0].name.as_str(), cookies[0].value.as_str()),
        ("renamed", "v2")
    );
}

#[gpui::test]
fn editing_a_hidden_value_keeps_it_unless_it_is_replaced(cx: &mut TestAppContext) {
    let dir = variables_workspace(closed_port());
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render_jar(cx, &dir, &state_path);
    let hidden = format!("{MASK}-kept");
    shell.update(cx, |shell, cx| {
        let crate::workspace::OpenedWorkspace::Loaded(engine) = &shell.workspace else {
            panic!("no workspace");
        };
        engine
            .set_cookie(&cookie("example.test", "token", &hidden, None))
            .unwrap_or_else(|e| panic!("{e}"));
        cx.notify();
    });
    cx.run_until_parked();

    click(cx, "cookie-1-edit");
    let opened = shell.read_with(cx, |shell, cx| {
        shell
            .settings
            .cookies
            .edit
            .as_ref()
            .map(|edit| edit.value.read(cx).value().to_string())
    });
    assert_eq!(
        opened.as_deref(),
        Some(""),
        "a hidden value is not put in the field"
    );
    type_into(cx, "cookie-edit-name", "token2");
    click(cx, "cookie-edit-save");

    let cookies = jar(&shell, cx);
    assert_eq!(
        (cookies[0].name.as_str(), cookies[0].value.as_str()),
        ("token2", hidden.as_str())
    );
}
