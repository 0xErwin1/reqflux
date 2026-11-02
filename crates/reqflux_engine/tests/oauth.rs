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
    OAuthFetch, OAuthTokenState, SecretKey, SecretScope, SecretStore,
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

#[test]
fn a_401_with_a_stored_token_drops_it_and_retries_once_with_a_new_one() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=client_credentials"))
                .respond_with(token(
                    json!({ "access_token": "fresh", "expires_in": 3600 }),
                )),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Bearer revoked"))
                .respond_with(ResponseTemplate::new(401)),
            Mock::given(method("GET"))
                .and(path("/api"))
                .and(header("authorization", "Bearer fresh"))
                .respond_with(api_ok()),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let revoked = json!({ "access_token": "revoked", "expires_at": 99_999_999_999u64 });
    if let Err(error) = store.set(&token_key(), &revoked.to_string()) {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    let executed = execute(&engine);

    assert_eq!(executed.response.status, 200);
    assert_eq!(received(&rt, &server, "/api").len(), 2);
    assert_eq!(received(&rt, &server, "/token").len(), 1);
    assert_eq!(stored(&store)["access_token"], "fresh");
}

#[test]
fn a_401_with_a_new_token_is_returned_and_the_next_send_fetches_another() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(token(
                    json!({ "access_token": "refused", "expires_in": 3600 }),
                )),
            Mock::given(method("GET"))
                .and(path("/api"))
                .respond_with(ResponseTemplate::new(401)),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    let first = execute(&engine);
    assert_eq!(first.response.status, 401);
    assert_eq!(
        received(&rt, &server, "/api").len(),
        1,
        "a token just fetched is not retried"
    );
    assert_eq!(
        store.get(&token_key()).ok().flatten(),
        None,
        "the refused token is dropped"
    );

    assert_eq!(execute(&engine).response.status, 401);
    assert_eq!(received(&rt, &server, "/token").len(), 2);
}

fn token_state(engine: &Engine) -> OAuthTokenState {
    match engine.oauth_token_state("api/r", None) {
        Ok(state) => state,
        Err(error) => panic!("token state: {error}"),
    }
}

fn fetch(engine: &Engine) -> OAuthFetch {
    match engine.fetch_oauth_token("api/r", None, &ExecuteOptions::default()) {
        Ok(fetch) => fetch,
        Err(error) => panic!("fetch: {error}"),
    }
}

fn finish(rt: &Runtime, fetch: OAuthFetch) -> Result<OAuthTokenState, HttpError> {
    match rt.block_on(fetch.handle) {
        Ok(outcome) => outcome,
        Err(error) => panic!("fetch task: {error}"),
    }
}

#[test]
fn the_token_state_reads_none_valid_and_expired_without_the_token() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    assert_eq!(token_state(&engine), OAuthTokenState::None);

    let valid = json!({ "access_token": "t", "expires_at": 99_999_999_999u64 });
    if let Err(error) = store.set(&token_key(), &valid.to_string()) {
        panic!("{error}");
    }
    assert_eq!(
        token_state(&engine),
        OAuthTokenState::Valid {
            expires_at: Some(99_999_999_999)
        }
    );

    let unbounded = json!({ "access_token": "t" });
    if let Err(error) = store.set(&token_key(), &unbounded.to_string()) {
        panic!("{error}");
    }
    assert_eq!(
        token_state(&engine),
        OAuthTokenState::Valid { expires_at: None }
    );

    let expired = json!({ "access_token": "t", "refresh_token": "r", "expires_at": 1000 });
    if let Err(error) = store.set(&token_key(), &expired.to_string()) {
        panic!("{error}");
    }
    assert_eq!(
        token_state(&engine),
        OAuthTokenState::Expired {
            expires_at: Some(1000),
            refreshable: true
        }
    );

    // A token a 401 dropped keeps only its refresh token, with no known expiry.
    let dropped = json!({ "access_token": "", "refresh_token": "r", "expires_at": 0 });
    if let Err(error) = store.set(&token_key(), &dropped.to_string()) {
        panic!("{error}");
    }
    assert_eq!(
        token_state(&engine),
        OAuthTokenState::Expired {
            expires_at: None,
            refreshable: true
        }
    );
}

#[test]
fn the_token_state_follows_a_draft_auth() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );
    let valid = json!({ "access_token": "t" });
    if let Err(error) = store.set(&token_key(), &valid.to_string()) {
        panic!("{error}");
    }
    let Some(collection) = engine.workspace().collections.first() else {
        panic!("no collection");
    };
    let mut draft = match collection.items.first() {
        Some(reqflux_core::model::Item::Request(request)) => request.request.clone(),
        _ => panic!("no request"),
    };
    draft.auth = reqflux_core::model::Auth::None;

    let state = match engine.oauth_token_state("api/r", Some(&draft)) {
        Ok(state) => state,
        Err(error) => panic!("{error}"),
    };

    assert_eq!(state, OAuthTokenState::None, "the draft has no OAuth 2.0");
}

#[test]
fn get_new_token_runs_the_grant_without_a_send_even_with_a_valid_token() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=client_credentials"))
                .respond_with(token(json!({ "access_token": "new", "expires_in": 3600 }))),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let valid = json!({ "access_token": "old", "expires_at": 99_999_999_999u64 });
    if let Err(error) = store.set(&token_key(), &valid.to_string()) {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    let fetch = fetch(&engine);
    assert_eq!(
        fetch.browser.redirect_uri(),
        None,
        "no browser for this grant"
    );
    let state = finish(&rt, fetch);

    assert!(
        matches!(
            state,
            Ok(OAuthTokenState::Valid {
                expires_at: Some(_)
            })
        ),
        "{state:?}"
    );
    assert_eq!(stored(&store)["access_token"], "new");
    assert!(
        received(&rt, &server, "/api").is_empty(),
        "nothing was sent"
    );
}

#[test]
fn get_new_token_tries_a_kept_refresh_token_first() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .and(body_string_contains("refresh_token=kept"))
                .respond_with(token(json!({ "access_token": "refreshed" }))),
        ],
    );
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let kept = json!({ "access_token": "old", "refresh_token": "kept", "expires_at": 1 });
    if let Err(error) = store.set(&token_key(), &kept.to_string()) {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::clone(&store),
    );

    let state = finish(&rt, fetch(&engine));

    assert_eq!(
        state.ok(),
        Some(OAuthTokenState::Valid { expires_at: None })
    );
    assert_eq!(stored(&store)["access_token"], "refreshed");
    assert_eq!(stored(&store)["refresh_token"], "kept");
    assert_eq!(received(&rt, &server, "/token").len(), 1);
}

#[test]
fn a_failed_get_new_token_keeps_the_previous_token() {
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
    let store = Arc::new(MemorySecretStore::new());
    let previous = json!({ "access_token": "previous", "expires_at": 99_999_999_999u64 });
    if let Err(error) = store.set(&token_key(), &previous.to_string()) {
        panic!("{error}");
    }
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\nclient_secret = \"literal-client-secret\"\n",
        Arc::clone(&store),
    );

    let error = match finish(&rt, fetch(&engine)) {
        Ok(state) => panic!("fetched: {state:?}"),
        Err(error) => error.to_string(),
    };

    assert_eq!(
        error,
        "authorization failed: the token endpoint answered 400 (invalid_client)"
    );
    assert_eq!(stored(&store)["access_token"], "previous");
}

#[test]
fn get_new_token_needs_an_oauth2_auth() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &server,
        "client_id = \"app\"\n",
        Arc::new(MemorySecretStore::new()),
    );
    let Some(collection) = engine.workspace().collections.first() else {
        panic!("no collection");
    };
    let mut draft = match collection.items.first() {
        Some(reqflux_core::model::Item::Request(request)) => request.request.clone(),
        _ => panic!("no request"),
    };
    draft.auth = reqflux_core::model::Auth::None;

    let error = match engine.fetch_oauth_token("api/r", Some(&draft), &ExecuteOptions::default()) {
        Ok(_) => panic!("a fetch started"),
        Err(error) => error.to_string(),
    };

    assert_eq!(
        error,
        "authorization failed: the request's auth is not OAuth 2.0"
    );
}

/// Records the authorization URL and never comes back, as a user who has not signed in yet.
#[derive(Debug, Default)]
struct WaitingBrowser {
    opened: Mutex<Vec<String>>,
}

impl Browser for WaitingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.opened
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url.to_owned());
        Ok(())
    }
}

#[test]
fn the_pkce_wait_shows_the_loopback_reopens_the_browser_and_cancels() {
    let rt = runtime();
    let server = start(&rt, Vec::new());
    let dir = temp_dir();
    let store = Arc::new(MemorySecretStore::new());
    let mut engine = engine(
        dir.path(),
        &server,
        &format!(
            "grant_type = \"authorization_code\"\nauth_url = \"{}/authorize\"\nclient_id = \"app\"\n",
            server.uri()
        ),
        Arc::clone(&store),
    );
    let browser = Arc::new(WaitingBrowser::default());
    engine.set_browser(browser.clone());

    let fetch = fetch(&engine);
    let mut redirect = None;
    for _ in 0..200 {
        redirect = fetch.browser.redirect_uri();
        if redirect.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let Some(redirect) = redirect else {
        panic!("the browser was not opened");
    };
    assert!(redirect.starts_with("http://127.0.0.1:"), "{redirect}");
    assert!(redirect.ends_with("/callback"), "{redirect}");

    if let Err(error) = fetch.browser.reopen() {
        panic!("{error}");
    }
    let opened = browser
        .opened
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(opened.len(), 2, "opened again");
    assert_eq!(opened.first(), opened.get(1), "the same authorization URL");

    fetch.cancel.cancel();
    let outcome = finish(&rt, fetch);
    assert!(matches!(outcome, Err(HttpError::Cancelled)), "{outcome:?}");
    assert!(store.accounts().is_empty(), "no token was stored");
    let port = redirect
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/callback")
        .to_owned();
    assert!(
        TcpStream::connect(format!("127.0.0.1:{port}")).is_err(),
        "the loopback listener closed"
    );
}
