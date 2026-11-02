//! OAuth 2.0 token acquisition against a local fake authorization server (v1-features S3):
//! client credentials, password, refresh and authorization code with PKCE through the
//! loopback redirect. Tokens live in an in-memory secret store, never the OS keyring.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{id, id_text, snapshot, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_engine::{
    Browser, Engine, ExecuteError, ExecuteOptions, Executed, HttpError, MASK, MemorySecretStore,
    SecretKey, SecretScope, SecretStore,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use tokio::runtime::Runtime;
use wiremock::matchers::{body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn runtime() -> Runtime {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("runtime: {error}"),
    }
}

fn start(runtime: &Runtime, mocks: Vec<Mock>) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        for mock in mocks {
            mock.mount(&server).await;
        }
        server
    })
}

fn received(runtime: &Runtime, server: &MockServer, at: &str) -> Vec<wiremock::Request> {
    match runtime.block_on(server.received_requests()) {
        Some(requests) => requests
            .into_iter()
            .filter(|request| request.url.path() == at)
            .collect(),
        None => panic!("request recording is disabled"),
    }
}

fn token(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn api_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_string("ok")
}

/// The key the token of request `api/r` is kept under.
fn token_key() -> SecretKey {
    SecretKey {
        workspace_id: id(900),
        scope: SecretScope::Collection(id(1)),
        key: format!("oauth2/request/{}", id_text(2)),
    }
}

fn stored(store: &MemorySecretStore) -> serde_json::Value {
    match store.get(&token_key()) {
        Ok(Some(text)) => match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) => panic!("stored token: {error}"),
        },
        other => panic!("no stored token: {other:?}"),
    }
}

/// Writes `api/r` (GET `<server>/api`) with an `oauth2` auth of `fields` and opens it.
fn engine(root: &Path, server: &MockServer, fields: &str, store: Arc<MemorySecretStore>) -> Engine {
    write(
        root,
        "reqflux.toml",
        &format!("id = \"{}\"\nname = \"W\"\nformat = 1\n", id_text(900)),
    );
    write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", id_text(1)),
    );
    write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"{uri}/api\"\n\n[auth]\ntype = \"oauth2\"\ntoken_url = \"{uri}/token\"\n{fields}",
            id_text(2),
            uri = server.uri(),
        ),
    );
    match Engine::open_with_secrets(root, store) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    }
}

fn execute(engine: &Engine) -> Executed {
    match engine.execute_blocking("api/r", &ExecuteOptions::default()) {
        Ok(executed) => executed,
        Err(error) => panic!("execute: {error}"),
    }
}

#[test]
fn client_credentials_token_is_kept_in_the_secret_store_and_reused() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=client_credentials"))
                .and(body_string_contains("client_id=app"))
                .and(body_string_contains("client_secret=literal-client-secret"))
                .and(body_string_contains("scope=read+write"))
                .respond_with(token(json!({
                    "access_token": "tok-1",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "refresh_token": "ref-1",
                }))),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Bearer tok-1"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\nclient_secret = \"literal-client-secret\"\nscope = \"read write\"\n",
        Arc::clone(&store),
    );
    let before = snapshot(dir.path());

    let first = execute(&engine);
    let second = execute(&engine);

    assert_eq!(first.response.status, 200);
    assert_eq!(second.response.status, 200);
    assert_eq!(
        received(&rt, &server, "/token").len(),
        1,
        "the token is reused"
    );
    let kept = stored(&store);
    assert_eq!(kept["access_token"], "tok-1");
    assert_eq!(kept["refresh_token"], "ref-1");
    assert!(kept["expires_at"].as_u64().is_some());
    assert_eq!(
        snapshot(dir.path()),
        before,
        "no token reaches a workspace file"
    );
    let recorded = format!("{:?}", first.sent);
    assert!(!recorded.contains("tok-1"), "{recorded}");
    assert!(!recorded.contains("literal-client-secret"), "{recorded}");
    assert!(
        recorded.contains(&format!("Bearer {MASK}")) || recorded.contains(MASK),
        "{recorded}"
    );
}

#[test]
fn password_grant_sends_the_token_in_the_query_and_never_records_it() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=password"))
                .and(body_string_contains("username=ana"))
                .and(body_string_contains("password=literal-pw"))
                .and(header("authorization", "Basic YXBwOnNlY3JldA=="))
                .respond_with(token(json!({ "access_token": "query-token" }))),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(query_param("token", "query-token"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &server,
        "grant_type = \"password\"\nclient_id = \"app\"\nclient_secret = \"secret\"\nusername = \"ana\"\npassword = \"literal-pw\"\nclient_auth = \"header\"\ntoken_location = \"query\"\nquery_param = \"token\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    let executed = execute(&engine);

    assert_eq!(executed.response.status, 200);
    assert!(!executed.response.final_url.contains("query-token"));
    assert_eq!(
        executed.sent.url,
        format!("{}/api?token={MASK}", server.uri()).replace(MASK, &url_encoded_mask())
    );
    assert!(!format!("{:?}", executed.sent).contains("literal-pw"));
}

fn url_encoded_mask() -> String {
    url::form_urlencoded::byte_serialize(MASK.as_bytes()).collect()
}

#[test]
fn an_expired_token_is_refreshed_before_the_send() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .and(body_string_contains("refresh_token=old-refresh"))
                .respond_with(token(json!({ "access_token": "fresh", "expires_in": 60 }))),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Bearer fresh"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let expired =
        json!({ "access_token": "stale", "refresh_token": "old-refresh", "expires_at": 1 });
    if let Err(error) = store.set(&token_key(), &expired.to_string()) {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    assert_eq!(execute(&engine).response.status, 200);
    let kept = stored(&store);
    assert_eq!(kept["access_token"], "fresh");
    assert_eq!(
        kept["refresh_token"], "old-refresh",
        "the refresh token is kept"
    );
}

#[test]
fn refresh_token_grant_uses_the_configured_refresh_token() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .and(body_string_contains("refresh_token=configured"))
                .respond_with(token(json!({ "access_token": "from-refresh" }))),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Token from-refresh"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &server,
        "grant_type = \"refresh_token\"\nrefresh_token = \"configured\"\nheader_prefix = \"Token\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    assert_eq!(execute(&engine).response.status, 200);
}

#[test]
fn a_token_endpoint_error_fails_the_send_without_any_credential() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(
                    ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_client" })),
                ),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\nclient_secret = \"literal-client-secret\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    let error = match engine.execute_blocking("api/r", &ExecuteOptions::default()) {
        Ok(executed) => panic!("sent: {executed:?}"),
        Err(error) => error,
    };

    assert!(
        matches!(error, ExecuteError::Http(HttpError::Auth(_))),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "authorization failed: the token endpoint answered 400 (invalid_client)"
    );
    assert!(received(&rt, &server, "/api").is_empty());
}

/// Follows the authorization redirect the way a browser would: a stray request first, then
/// the callback with `code` and the `state` it was given, or `state` when set.
#[derive(Debug, Default)]
struct FollowingBrowser {
    state: Option<String>,
    opened: Mutex<Vec<String>>,
}

impl Browser for FollowingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.opened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url.to_owned());
        let parsed = url::Url::parse(url).map_err(|error| error.to_string())?;
        let param = |name: &str| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default()
        };
        let redirect =
            url::Url::parse(&param("redirect_uri")).map_err(|error| error.to_string())?;
        let address = format!(
            "{}:{}",
            redirect.host_str().unwrap_or_default(),
            redirect.port().unwrap_or_default()
        );
        let state = self.state.clone().unwrap_or_else(|| param("state"));
        std::thread::spawn(move || {
            hit(&address, "/favicon.ico");
            hit(&address, &format!("/callback?code=the-code&state={state}"));
        });
        Ok(())
    }
}

fn hit(address: &str, target: &str) {
    let Ok(mut stream) = TcpStream::connect(address) else {
        return;
    };
    let request = format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_ok() {
        let mut answer = Vec::new();
        drop(stream.read_to_end(&mut answer));
    }
}

#[test]
fn authorization_code_with_pkce_follows_the_loopback_redirect() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=authorization_code"))
                .and(body_string_contains("code=the-code"))
                .respond_with(token(
                    json!({ "access_token": "code-token", "expires_in": 600 }),
                )),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Bearer code-token"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = engine(
        dir.path(),
        &server,
        &format!(
            "grant_type = \"authorization_code\"\nauth_url = \"{}/authorize\"\nclient_id = \"app\"\nscope = \"openid\"\n",
            server.uri()
        ),
        Arc::clone(&store),
    );
    let browser = Arc::new(FollowingBrowser::default());
    engine.set_browser(browser.clone());

    assert_eq!(execute(&engine).response.status, 200);

    let opened = browser
        .opened
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some(Ok(authorize)) = opened.first().map(|url| url::Url::parse(url)) else {
        panic!("the browser was not opened: {opened:?}");
    };
    let param = |name: &str| {
        authorize
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default()
    };
    assert_eq!(authorize.path(), "/authorize");
    assert_eq!(param("response_type"), "code");
    assert_eq!(param("client_id"), "app");
    assert_eq!(param("scope"), "openid");
    assert_eq!(param("code_challenge_method"), "S256");
    assert!(param("redirect_uri").starts_with("http://127.0.0.1:"));
    assert!(param("redirect_uri").ends_with("/callback"));
    let exchanges = received(&rt, &server, "/token");
    let Some(exchange) = exchanges.first() else {
        panic!("no code exchange");
    };
    let form: Vec<(String, String)> = url::form_urlencoded::parse(&exchange.body)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let verifier = field("code_verifier");
    assert!(verifier.len() >= 43, "{verifier}");
    assert_eq!(
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        param("code_challenge")
    );
    assert_eq!(field("redirect_uri"), param("redirect_uri"));
    assert_eq!(stored(&store)["access_token"], "code-token");
}

#[test]
fn a_redirect_with_the_wrong_state_fails_the_authorization() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let mut engine = engine(
        dir.path(),
        &server,
        &format!(
            "grant_type = \"authorization_code\"\nauth_url = \"{}/authorize\"\nclient_id = \"app\"\n",
            server.uri()
        ),
        Arc::new(MemorySecretStore::new()),
    );
    engine.set_browser(Arc::new(FollowingBrowser {
        state: Some("forged".to_owned()),
        ..FollowingBrowser::default()
    }));

    let error = match engine.execute_blocking("api/r", &ExecuteOptions::default()) {
        Ok(executed) => panic!("sent: {executed:?}"),
        Err(error) => error.to_string(),
    };

    assert_eq!(
        error,
        "authorization failed: the authorization redirect has the wrong state"
    );
    assert!(received(&rt, &server, "/token").is_empty());
}

#[test]
fn forgetting_the_token_deletes_it_from_the_secret_store() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    if let Err(error) = store.set(&token_key(), "{\"access_token\":\"t\"}") {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    if let Err(error) = engine.forget_oauth_token("api/r") {
        panic!("{error}");
    }

    assert!(store.accounts().is_empty());
}
