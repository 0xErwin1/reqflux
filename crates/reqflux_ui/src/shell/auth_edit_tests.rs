//! The Auth pane edits the draft (S3): the type menu, the plain and secret fields and the
//! API key location, and their persistence (S8).

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{ApiKeyLocation, Auth};
use tempfile::TempDir;

use super::Shell;
use super::draft_tests::{bounds, click, marker, present, render, state_file};
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000071";
const GUARDED: &str = "01J00000000000000000000072";
const FILE: &str = "collections/api/guarded.req.toml";

/// A workspace holding `GET Guarded`, whose auth is the `[auth]` table `auth`.
fn auth_workspace(auth: &str) -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{COLLECTION}\"\nname = \"API\"\n"),
    );
    write(
        root,
        FILE,
        &format!(
            "id = \"{GUARDED}\"\nname = \"Guarded\"\nmethod = \"GET\"\nurl = \"https://h/g\"\n\n\
             [auth]\n{auth}"
        ),
    );
    dir
}

fn bearer_workspace() -> TempDir {
    auth_workspace("type = \"bearer\"\ntoken = \"stored-token\"\n")
}

/// Opens `GET Guarded` on its Auth sub-tab.
fn open_auth(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{GUARDED}"));
    click(cx, "request-tab-auth");
}

fn draft_auth(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<Auth> {
    cx.update(|cx| {
        shell
            .read(cx)
            .drafts
            .get(GUARDED)
            .map(|draft: &super::drafts::RequestDraft| draft.request.auth.clone())
    })
}

/// The text and placeholder of the open field, if it is open.
fn field(cx: &mut TestAppContext, shell: &Entity<Shell>) -> Option<(String, String)> {
    cx.update(|cx| {
        shell.read(cx).cell_edit.as_ref().map(|edit| {
            let input = edit.input.read(cx);
            (
                input.value().to_string(),
                input.presentation().placeholder().to_string(),
            )
        })
    })
}

/// Picks the auth type `kind` from the type menu.
fn pick_type(cx: &mut VisualTestContext, kind: &str) {
    click(cx, "request-auth-type");
    click(cx, &format!("auth-type-{kind}"));
}

/// Opens the field of the row `label`, replaces its text with `text` and presses `key`.
fn type_field(cx: &mut VisualTestContext, label: &str, text: &str, key: &str) {
    click(cx, &format!("request-auth-field-{label}"));
    assert!(
        bounds(cx, "request-auth-input").is_some(),
        "the field opened"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
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

fn file(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default()
}

#[gpui::test]
fn the_type_menu_switches_the_type_in_the_draft(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    click(cx, "request-auth-type");
    for kind in ["none", "basic", "bearer", "apikey"] {
        assert!(bounds(cx, &format!("auth-type-{kind}")).is_some(), "{kind}");
    }
    assert!(
        bounds(cx, "auth-type-inherit").is_none(),
        "inheriting is the Source control's"
    );
    click(cx, "auth-type-bearer");
    assert!(
        bounds(cx, "auth-type-bearer").is_none(),
        "the pick closed it"
    );
    assert!(draft_auth(cx, &shell).is_none(), "the same type is a no-op");

    pick_type(cx, "apikey");
    assert_eq!(
        draft_auth(cx, &shell),
        Some(Auth::ApiKey {
            key: String::new(),
            value: String::new(),
            location: ApiKeyLocation::Header,
        })
    );
    assert!(marker(cx, GUARDED), "the edit marks the tab");

    click(cx, "request-auth-type");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(bounds(cx, "auth-type-none").is_none(), "Escape closes it");

    let (shell_2, cx) = render(cx, &dir, &state_path);
    assert!(
        matches!(draft_auth(cx, &shell_2), Some(Auth::ApiKey { .. })),
        "the draft was restored"
    );
}

#[gpui::test]
fn the_source_control_switches_between_inherited_and_own_auth(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert_eq!(label(cx, "auth-source-inherit"), "Inherited from API");
    assert_eq!(label(cx, "auth-source-request"), "This request");
    click(cx, "auth-source-request");
    assert!(
        draft_auth(cx, &shell).is_none(),
        "the same source is a no-op"
    );

    click(cx, "auth-source-inherit");
    assert_eq!(draft_auth(cx, &shell), Some(Auth::Inherit));
    assert!(
        bounds(cx, "request-auth-type").is_none(),
        "an inherited auth has no type of its own"
    );
    assert_eq!(
        label(cx, "request-auth-hint"),
        "This request sends the auth of API."
    );

    click(cx, "auth-source-request");
    assert_eq!(draft_auth(cx, &shell), Some(Auth::None));
    assert!(bounds(cx, "request-auth-type").is_some());
}

#[gpui::test]
fn a_secret_shows_masked_in_a_field(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert!(
        bounds(cx, "request-auth-well-token").is_some(),
        "the token sits in a field well"
    );
    let shown = label(cx, "request-auth-field-token");
    assert_eq!(shown, "Token: ••••••••");
    assert!(!shown.contains("stored-token"));
}

#[gpui::test]
fn username_and_key_edit_the_draft_and_add_to_toggles(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    pick_type(cx, "basic");
    type_field(cx, "username", "ana", "enter");
    assert!(
        bounds(cx, "request-auth-input").is_none(),
        "Enter closed it"
    );
    assert_eq!(
        draft_auth(cx, &shell),
        Some(Auth::Basic {
            username: "ana".to_owned(),
            password: String::new(),
        })
    );
    click(cx, "request-auth-field-username");
    assert_eq!(
        field(cx, &shell).map(|(text, _)| text).as_deref(),
        Some("ana")
    );
    cx.simulate_keystrokes("escape");

    pick_type(cx, "apikey");
    type_field(cx, "key", "X-Key", "enter");
    click(cx, "auth-location-query");
    let Some(Auth::ApiKey { key, location, .. }) = draft_auth(cx, &shell) else {
        panic!("an API key draft");
    };
    assert_eq!((key.as_str(), location), ("X-Key", ApiKeyLocation::Query));
    click(cx, "auth-location-query");
    assert!(
        matches!(
            draft_auth(cx, &shell),
            Some(Auth::ApiKey {
                location: ApiKeyLocation::Query,
                ..
            })
        ),
        "the selected location is a no-op"
    );
    click(cx, "auth-location-header");
    assert!(matches!(
        draft_auth(cx, &shell),
        Some(Auth::ApiKey {
            location: ApiKeyLocation::Header,
            ..
        })
    ));
}

#[gpui::test]
fn a_secret_field_opens_empty_and_an_empty_commit_keeps_it(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    click(cx, "request-auth-field-token");
    assert_eq!(
        field(cx, &shell),
        Some((String::new(), "Secret value".to_owned())),
        "never seeded"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(draft_auth(cx, &shell).is_none(), "the stored token is kept");

    type_field(cx, "token", "abc", "escape");
    assert!(bounds(cx, "request-auth-input").is_none());
    assert!(draft_auth(cx, &shell).is_none(), "Escape discards");
    assert!(!marker(cx, GUARDED));

    type_field(cx, "token", "fresh", "enter");
    assert_eq!(
        draft_auth(cx, &shell),
        Some(Auth::Bearer {
            token: "fresh".to_owned()
        })
    );

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        file(&dir).contains("token = \"fresh\""),
        "Ctrl+S wrote it: {}",
        file(&dir)
    );
}

#[gpui::test]
fn an_unsupported_type_stays_read_only(cx: &mut TestAppContext) {
    let dir = auth_workspace("type = \"oauth2\"\nclient_secret = \"hidden\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert!(bounds(cx, "request-auth-unsupported").is_some());
    click(cx, "request-auth-field-client_secret");
    assert!(bounds(cx, "request-auth-input").is_none(), "no field opens");
    assert!(field(cx, &shell).is_none());
    assert!(draft_auth(cx, &shell).is_none());
}

#[gpui::test]
fn switching_the_sub_tab_closes_the_type_menu(cx: &mut TestAppContext) {
    let dir = bearer_workspace();
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    click(cx, "request-auth-type");
    assert!(bounds(cx, "auth-type-none").is_some(), "the menu is open");
    click(cx, "request-tab-headers");
    click(cx, "request-tab-auth");
    assert!(
        bounds(cx, "auth-type-none").is_none(),
        "the menu does not outlive its pane"
    );
    assert!(cx.update(|_, cx| !shell.read(cx).auth_menu_open));
}
