//! The Auth pane edits digest, AWS SigV4 and OAuth 2.0 (v1-features S3): fields write the
//! canonical keys through the typed views, secrets stay masked while region and client id
//! show, the selects pick the algorithm and the grant, and the OAuth 2.0 Token row shows the
//! token's state and runs Get new token and Clear on the engine, never showing the token.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::{
    Auth, AwsV4Auth, DigestAlgorithm, DigestAuth, OAuth2Auth, OAuth2Grant, TypedAuth,
};
use reqflux_engine::Browser;
use tempfile::TempDir;

use crate::shell::Shell;
use crate::shell::draft_tests::{bounds, click, present, render, state_file};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const COLLECTION: &str = "01J00000000000000000000081";
const GUARDED: &str = "01J00000000000000000000082";
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

fn open_auth(cx: &mut VisualTestContext) {
    click(cx, &format!("tree-row-{GUARDED}"));
    click(cx, "request-tab-auth");
}

fn draft_auth(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<Auth> {
    cx.update(|_, cx| {
        shell
            .read(cx)
            .drafts
            .get(GUARDED)
            .map(|draft| draft.request.auth.clone())
    })
}

fn typed(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<TypedAuth> {
    draft_auth(cx, shell)
        .and_then(|auth| auth.typed())
        .and_then(Result::ok)
}

fn pick_type(cx: &mut VisualTestContext, kind: &str) {
    click(cx, "request-auth-type");
    click(cx, &format!("auth-type-{kind}"));
}

/// Opens the field of the row `label`, types `text` and presses Enter.
fn type_field(cx: &mut VisualTestContext, label: &str, text: &str) {
    click(cx, &format!("request-auth-field-{label}"));
    assert!(
        bounds(cx, "request-auth-input").is_some(),
        "the {label} field opened"
    );
    cx.simulate_keystrokes("ctrl-a backspace");
    for character in text.chars() {
        cx.simulate_keystrokes(&character.to_string());
    }
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

/// The text and placeholder of the open field.
fn field(cx: &mut VisualTestContext, shell: &Entity<Shell>) -> Option<(String, String)> {
    cx.update(|_, cx| {
        shell.read(cx).cell_edit.as_ref().map(|edit| {
            let input = edit.input.read(cx);
            (
                input.value().to_string(),
                input.presentation().placeholder().to_string(),
            )
        })
    })
}

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

/// Every accessible label on screen, joined.
fn all_labels(cx: &mut VisualTestContext) -> String {
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| {
        automation.attach(window);
        window.refresh();
    });
    cx.run_until_parked();
    automation
        .snapshot()
        .nodes
        .values()
        .filter_map(|node| node.label.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn file(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default()
}

#[gpui::test]
fn digest_fields_write_the_canonical_keys(cx: &mut TestAppContext) {
    let dir = auth_workspace("type = \"none\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    pick_type(cx, "digest");
    assert_eq!(draft_auth(cx, &shell), Some(DigestAuth::default().into()));
    assert_eq!(label(cx, "request-auth-type"), "Digest");

    type_field(cx, "username", "bot");
    click(cx, "request-auth-field-password");
    assert_eq!(
        field(cx, &shell),
        Some((String::new(), "Secret value".to_owned())),
        "a secret opens empty"
    );
    cx.simulate_keystrokes("escape");
    type_field(cx, "password", "digest-literal");
    click(cx, "request-auth-algorithm");
    click(cx, "auth-algorithm-sha-256");
    assert!(
        bounds(cx, "auth-algorithm-menu").is_none(),
        "the pick closed it"
    );

    assert_eq!(
        typed(cx, &shell),
        Some(TypedAuth::Digest(DigestAuth {
            username: "bot".to_owned(),
            password: "digest-literal".to_owned(),
            algorithm: DigestAlgorithm::Sha256,
        }))
    );
    assert_eq!(
        label(cx, "request-auth-field-password"),
        "Password: ••••••••"
    );
    assert_eq!(
        label(cx, "request-auth-field-algorithm"),
        "Algorithm: SHA-256"
    );
    assert!(!all_labels(cx).contains("digest-literal"));

    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    assert!(
        file(&dir).contains(
            "[auth]\ntype = \"digest\"\nusername = \"bot\"\npassword = \"digest-literal\"\nalgorithm = \"SHA-256\"\n"
        ),
        "{}",
        file(&dir)
    );
}

#[gpui::test]
fn aws_shows_region_and_service_and_masks_the_keys(cx: &mut TestAppContext) {
    let dir = auth_workspace(
        "type = \"awsv4\"\naccess_key = \"AKIDEXAMPLE\"\nsecret_key = \"aws-literal\"\nregion = \"eu-west-1\"\nservice = \"execute-api\"\n",
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert_eq!(label(cx, "request-auth-field-region"), "Region: eu-west-1");
    assert_eq!(
        label(cx, "request-auth-field-service"),
        "Service: execute-api"
    );
    assert_eq!(
        label(cx, "request-auth-field-access-key"),
        "Access key: AKIDEXAMPLE"
    );
    assert_eq!(
        label(cx, "request-auth-field-secret-key"),
        "Secret key: ••••••••"
    );
    assert_eq!(
        label(cx, "request-auth-field-session-token"),
        "Session token: Optional"
    );
    assert!(!all_labels(cx).contains("aws-literal"));
    assert!(bounds(cx, "request-auth-unsupported").is_none());

    type_field(cx, "region", "us-east-1");
    let Some(TypedAuth::AwsV4(AwsV4Auth {
        region, secret_key, ..
    })) = typed(cx, &shell)
    else {
        panic!("an AWS draft");
    };
    assert_eq!(
        (region.as_str(), secret_key.as_str()),
        ("us-east-1", "aws-literal")
    );
}

#[gpui::test]
fn editing_a_field_keeps_unknown_keys_and_postman_spellings(cx: &mut TestAppContext) {
    let dir = auth_workspace(
        "type = \"awsv4\"\naccessKey = \"AKIDEXAMPLE\"\nsecret_key = \"aws-literal\"\nregion = \"eu-west-1\"\nservice = \"s3\"\nx_custom = \"kept\"\n",
    );
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    type_field(cx, "region", "us-east-1");
    type_field(cx, "access-key", "AKIDNEW");
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();

    assert!(
        file(&dir).contains(
            "[auth]\ntype = \"awsv4\"\naccessKey = \"AKIDNEW\"\nsecret_key = \"aws-literal\"\nregion = \"us-east-1\"\nservice = \"s3\"\nx_custom = \"kept\"\n"
        ),
        "{}",
        file(&dir)
    );
    assert!(!file(&dir).contains("access_key"), "{}", file(&dir));
}

#[gpui::test]
fn the_grant_select_changes_the_oauth_rows(cx: &mut TestAppContext) {
    let dir = auth_workspace(
        "type = \"oauth2\"\ntoken_url = \"https://id/token\"\nclient_id = \"billing\"\nclient_secret = \"oauth-literal\"\n",
    );
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert_eq!(label(cx, "request-auth-type"), "OAuth 2.0");
    assert_eq!(
        label(cx, "request-auth-field-client-id"),
        "Client ID: billing"
    );
    assert_eq!(
        label(cx, "request-auth-field-client-secret"),
        "Client secret: ••••••••"
    );
    assert!(bounds(cx, "request-auth-field-auth-url").is_none());
    assert!(!all_labels(cx).contains("oauth-literal"));

    click(cx, "request-auth-grant");
    for id in [
        "client-credentials",
        "authorization-code",
        "password",
        "refresh-token",
    ] {
        assert!(bounds(cx, &format!("auth-grant-{id}")).is_some(), "{id}");
    }
    click(cx, "auth-grant-authorization-code");
    assert!(bounds(cx, "request-auth-field-auth-url").is_some());
    type_field(cx, "auth-url", "https://id/authorize");

    let Some(TypedAuth::OAuth2(oauth)) = typed(cx, &shell) else {
        panic!("an OAuth 2.0 draft");
    };
    assert_eq!(
        *oauth,
        OAuth2Auth {
            grant_type: OAuth2Grant::AuthorizationCode,
            token_url: "https://id/token".to_owned(),
            auth_url: "https://id/authorize".to_owned(),
            client_id: "billing".to_owned(),
            client_secret: "oauth-literal".to_owned(),
            ..OAuth2Auth::default()
        }
    );
}

#[gpui::test]
fn basic_bearer_and_api_key_still_edit_as_before(cx: &mut TestAppContext) {
    let dir = auth_workspace("type = \"bearer\"\ntoken = \"stored-token\"\n");
    let (_state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    type_field(cx, "token", "fresh");
    assert_eq!(
        draft_auth(cx, &shell),
        Some(Auth::Bearer {
            token: "fresh".to_owned()
        })
    );
    pick_type(cx, "basic");
    type_field(cx, "username", "ana");
    type_field(cx, "password", "pw");
    assert_eq!(
        draft_auth(cx, &shell),
        Some(Auth::Basic {
            username: "ana".to_owned(),
            password: "pw".to_owned(),
        })
    );
    pick_type(cx, "apikey");
    type_field(cx, "key", "X-Key");
    type_field(cx, "value", "v");
    let Some(Auth::ApiKey { key, value, .. }) = draft_auth(cx, &shell) else {
        panic!("an API key draft");
    };
    assert_eq!((key.as_str(), value.as_str()), ("X-Key", "v"));
    assert_eq!(label(cx, "request-auth-field-value"), "Value: ••••••••");
}

/// A token endpoint that answers every request with a fresh token; returns its port.
fn serve_tokens() -> u16 {
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
            let mut received = Vec::new();
            let mut byte = [0_u8; 1];
            while !received.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(1) => received.extend_from_slice(&byte),
                    Ok(_) | Err(_) => break,
                }
            }
            let head = String::from_utf8_lossy(&received).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut body = vec![0_u8; length];
            if let Err(error) = stream.read_exact(&mut body) {
                eprintln!("read: {error}");
                continue;
            }
            let text = "{\"access_token\":\"ui-token-literal\",\"expires_in\":3600}";
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{text}",
                text.len()
            );
            if let Err(error) = stream.write_all(reply.as_bytes()) {
                eprintln!("write: {error}");
            }
        }
    });
    port
}

/// A browser that never comes back, as a user who has not signed in yet.
#[derive(Debug)]
struct SilentBrowser;

impl Browser for SilentBrowser {
    fn open(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Waits until the Token row reads `expected`.
fn wait_for_token(cx: &mut VisualTestContext, expected: &str) -> String {
    let mut shown = String::new();
    // Up to ten seconds of wall time: the engine fetches on its own threads.
    for _ in 0..1000 {
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        shown = label(cx, "request-auth-token");
        if shown.starts_with(expected) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    shown
}

#[gpui::test]
fn get_new_token_stores_a_token_and_clear_drops_it(cx: &mut TestAppContext) {
    let port = serve_tokens();
    let dir = auth_workspace(&format!(
        "type = \"oauth2\"\ntoken_url = \"http://127.0.0.1:{port}/token\"\nclient_id = \"app\"\n"
    ));
    let (_state_dir, state_path) = state_file();
    let (_shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);

    assert_eq!(label(cx, "request-auth-token"), "Token: No token");
    assert_eq!(label(cx, "request-auth-token-state"), "No token");
    assert!(bounds(cx, "request-auth-token-clear").is_none());

    click(cx, "request-auth-token-get");
    let shown = wait_for_token(cx, "Token: Valid");
    assert!(shown.starts_with("Token: Valid until "), "{shown}");
    assert!(shown.ends_with(" left"), "{shown}");
    assert!(
        !all_labels(cx).contains("ui-token-literal"),
        "the token is never shown"
    );

    click(cx, "request-auth-token-clear");
    assert_eq!(label(cx, "request-auth-token"), "Token: No token");
}

#[gpui::test]
fn the_pkce_wait_locks_the_form_until_cancel(cx: &mut TestAppContext) {
    let (_dir, _state_dir, _shell, cx) = pkce(cx);

    click(cx, "request-auth-token-get");
    let waiting = wait_until_locked(cx);
    assert!(
        waiting.starts_with(
            "Waiting for the browser. Sign in at id.example. ReqFlux listens on http://127.0.0.1:"
        ),
        "{waiting}"
    );
    assert!(waiting.ends_with("/callback for 5 minutes."), "{waiting}");

    click(cx, "request-auth-field-client-id");
    assert!(
        bounds(cx, "request-auth-input").is_none(),
        "the form is locked"
    );

    click(cx, "request-auth-token-cancel");
    assert!(bounds(cx, "request-auth-token-waiting").is_none());
    assert_eq!(label(cx, "request-auth-token"), "Token: No token");
    click(cx, "request-auth-field-client-id");
    assert!(bounds(cx, "request-auth-input").is_some(), "unlocked");
}

#[gpui::test]
fn a_menu_open_when_the_form_locks_closes_with_it(cx: &mut TestAppContext) {
    let (_dir, _state_dir, shell, cx) = pkce(cx);
    click(cx, "request-auth-type");
    present(cx, "auth-type-menu");

    // The click that starts the fetch would close the menu through its own backdrop, so the
    // fetch starts here as the Token row's button starts it.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.get_new_token(GUARDED, window, cx));
    });
    let waiting = wait_until_locked(cx);
    assert!(waiting.starts_with("Waiting for the browser."), "{waiting}");
    assert!(
        bounds(cx, "auth-type-menu").is_none(),
        "the locked form draws no menu over its rows"
    );

    click(cx, "request-auth-field-client-id");
    assert!(
        bounds(cx, "request-auth-input").is_none(),
        "a locked row takes no click"
    );
    click(cx, "request-auth-token-cancel");
    // The lock closed the menu, it did not only hide it: unlocked, it stays closed.
    cx.run_until_parked();
    assert!(
        bounds(cx, "auth-type-menu").is_none(),
        "the menu stays closed once the form unlocks"
    );
}

/// The shell over an OAuth 2.0 authorization code request whose browser never comes back, its
/// Auth pane open, with the state directory the shell keeps its file in.
fn pkce(cx: &mut TestAppContext) -> (TempDir, TempDir, Entity<Shell>, &mut VisualTestContext) {
    let dir = auth_workspace(
        "type = \"oauth2\"\ngrant_type = \"authorization_code\"\nauth_url = \"https://id.example/authorize\"\ntoken_url = \"https://id.example/token\"\nclient_id = \"app\"\n",
    );
    let (state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    open_auth(cx);
    // The test never opens a real browser: this one does not come back.
    cx.update(|_, cx| {
        shell.update(cx, |shell, _| {
            let OpenedWorkspace::Loaded(engine) = &mut shell.workspace else {
                panic!("no workspace");
            };
            let Some(engine) = Arc::get_mut(engine) else {
                panic!("the engine is shared");
            };
            engine.set_browser(Arc::new(SilentBrowser));
        });
    });
    (dir, state_dir, shell, cx)
}

/// Waits for Get new token to lock the form and returns the line the Token row shows.
fn wait_until_locked(cx: &mut VisualTestContext) -> String {
    for _ in 0..1000 {
        // The pane redraws on a timer while the fetch runs.
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        if bounds(cx, "request-auth-token-reopen").is_some() {
            return label(cx, "request-auth-token-waiting");
        }
        thread::sleep(Duration::from_millis(10));
    }
    all_labels(cx)
}
