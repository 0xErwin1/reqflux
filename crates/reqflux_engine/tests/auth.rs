//! Digest and AWS SigV4 through the send path, and the masking of every credential of the
//! typed auth kinds (v1-features S3).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{id_text, post, single, temp_dir, write};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Auth, AwsV4Auth, DigestAuth, OAuth2Auth, OAuth2Grant};
use reqflux_engine::{Engine, ExecuteOptions, Executed, MASK, MemorySecretStore};
use reqflux_http::auth::digest::{self, DigestHash, DigestInput};
use tokio::runtime::Runtime;
use wiremock::matchers::{header_exists, method, path};
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

fn received(runtime: &Runtime, server: &MockServer) -> Vec<wiremock::Request> {
    match runtime.block_on(server.received_requests()) {
        Some(requests) => requests,
        None => panic!("request recording is disabled"),
    }
}

/// Writes a workspace with the collection `api` and the request `api/r` whose `[auth]` is
/// `auth`, and opens it on `store`.
fn engine(root: &Path, url: &str, auth: &str, store: Arc<MemorySecretStore>) -> Engine {
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
            "id = \"{}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"{url}\"\n\n{auth}",
            id_text(2)
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

fn header<'a>(request: &'a wiremock::Request, name: &str) -> &'a str {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

/// The quoted or bare value of `key` in a digest header.
fn param(header: &str, key: &str) -> String {
    header
        .split(", ")
        .find_map(|part| part.strip_prefix(&format!("{key}=")))
        .map(|value| value.trim_matches('"').to_owned())
        .unwrap_or_default()
}

#[test]
fn digest_answers_the_401_challenge_once() {
    let rt = runtime();
    let challenge = "Digest realm=\"http-auth@example.org\", qop=\"auth, auth-int\", algorithm=SHA-256, nonce=\"7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v\", opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\"";
    let server = start(
        &rt,
        vec![
            Mock::given(method("GET"))
                .and(path("/dir/index.html"))
                .and(header_exists("authorization"))
                .respond_with(ResponseTemplate::new(200).set_body_string("in"))
                .with_priority(1),
            Mock::given(method("GET"))
                .and(path("/dir/index.html"))
                .respond_with(
                    ResponseTemplate::new(401).insert_header("WWW-Authenticate", challenge),
                )
                .with_priority(2),
        ],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &format!("{}/dir/index.html", server.uri()),
        "[auth]\ntype = \"digest\"\nusername = \"Mufasa\"\npassword = \"Circle of Life\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    let executed = execute(&engine);

    assert_eq!(executed.response.status, 200);
    assert_eq!(executed.response.body, b"in");
    let requests = received(&rt, &server);
    assert_eq!(requests.len(), 2);
    let sent = requests
        .get(1)
        .map(|request| header(request, "authorization"));
    let sent = sent.unwrap_or_default();
    // The RFC 7616 computation with the cnonce the engine chose.
    let Some(parsed) = digest::challenge(&[("www-authenticate".to_owned(), challenge.to_owned())])
    else {
        panic!("challenge");
    };
    let cnonce = param(sent, "cnonce");
    let expected = digest::authorization(
        &parsed,
        &DigestInput {
            method: "GET",
            uri: "/dir/index.html",
            username: "Mufasa",
            password: "Circle of Life",
            fallback: DigestHash::Md5,
            cnonce: &cnonce,
            nc: 1,
        },
    );
    assert_eq!(Ok(sent.to_owned()), expected);
    assert_eq!(param(sent, "nc"), "00000001");
    assert!(!format!("{:?}", executed.sent).contains("Circle of Life"));
}

#[test]
fn digest_without_a_challenge_returns_the_first_response() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(401))],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &server.uri(),
        "[auth]\ntype = \"digest\"\nusername = \"u\"\npassword = \"p\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    assert_eq!(execute(&engine).response.status, 401);
    assert_eq!(received(&rt, &server).len(), 1);
}

#[test]
fn aws_sigv4_signs_the_sent_request_and_masks_the_secret_key_and_session_token() {
    let rt = runtime();
    let server = start(
        &rt,
        vec![Mock::given(method("GET")).respond_with(ResponseTemplate::new(200))],
    );
    let dir = temp_dir();
    let engine = engine(
        dir.path(),
        &format!("{}/v1/items?b=2&a=1", server.uri()),
        "[auth]\ntype = \"awsv4\"\naccess_key = \"AKIDEXAMPLE\"\nsecret_key = \"literal-aws-secret\"\nsession_token = \"literal-session-token\"\nregion = \"us-east-1\"\nservice = \"execute-api\"\n",
        Arc::new(MemorySecretStore::new()),
    );

    let executed = execute(&engine);

    let requests = received(&rt, &server);
    let Some(first) = requests.first() else {
        panic!("nothing sent");
    };
    let authorization = header(first, "authorization");
    assert!(
        authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"),
        "{authorization}"
    );
    assert!(
        authorization.contains("/us-east-1/execute-api/aws4_request, SignedHeaders=host;x-amz-date;x-amz-security-token, Signature="),
        "{authorization}"
    );
    assert_eq!(
        header(first, "x-amz-security-token"),
        "literal-session-token"
    );
    assert_eq!(header(first, "x-amz-date").len(), 16);
    let recorded = format!("{:?}", executed.sent);
    for secret in ["literal-aws-secret", "literal-session-token", "Signature="] {
        assert!(!recorded.contains(secret), "{recorded}");
    }
    assert!(recorded.contains("X-Amz-Date"), "{recorded}");
}

/// Each typed kind with a literal credential in every secret field: none reaches the
/// prepared request as history shows it, the history of a send, or the workspace masker.
#[test]
fn literal_credentials_of_every_typed_kind_are_masked() {
    let kinds: Vec<(Auth, Vec<&str>)> = vec![
        (
            DigestAuth {
                username: "user".to_owned(),
                password: "digest-literal-password".to_owned(),
                ..DigestAuth::default()
            }
            .into(),
            vec!["digest-literal-password"],
        ),
        (
            AwsV4Auth {
                access_key: "AKID".to_owned(),
                secret_key: "aws-literal-secret".to_owned(),
                session_token: "aws-literal-session".to_owned(),
                region: "us-east-1".to_owned(),
                service: "s3".to_owned(),
            }
            .into(),
            vec!["aws-literal-secret", "aws-literal-session"],
        ),
        (
            OAuth2Auth {
                grant_type: OAuth2Grant::Password,
                token_url: "http://127.0.0.1:9/token".to_owned(),
                client_id: "client".to_owned(),
                client_secret: "oauth-literal-client-secret".to_owned(),
                username: "user".to_owned(),
                password: "oauth-literal-password".to_owned(),
                refresh_token: "oauth-literal-refresh".to_owned(),
                ..OAuth2Auth::default()
            }
            .into(),
            vec![
                "oauth-literal-client-secret",
                "oauth-literal-password",
                "oauth-literal-refresh",
            ],
        ),
    ];
    for (auth, secrets) in kinds {
        let mut doc = post("https://api.test/v1");
        doc.headers = secrets
            .iter()
            .enumerate()
            .map(|(n, secret)| reqflux_core::model::KeyValue::new(format!("X-Echo-{n}"), *secret))
            .collect();
        doc.auth = auth.clone();
        let workspace = single(doc);
        let sent = format!("{:?}", common::prepare(&workspace).sent());
        for secret in &secrets {
            assert!(
                !sent.contains(secret),
                "{} leaked {secret}: {sent}",
                auth.kind()
            );
        }
        assert!(sent.contains(MASK), "{sent}");

        let dir = temp_dir();
        let engine = engine(
            dir.path(),
            "http://127.0.0.1:9/",
            &toml_auth(&auth),
            Arc::new(MemorySecretStore::new()),
        );
        let (masker, _) = engine.workspace_masker();
        for secret in &secrets {
            assert_eq!(masker.mask(secret), MASK, "{}", auth.kind());
        }
        assert_eq!(
            masker.mask("us-east-1 user client"),
            "us-east-1 user client"
        );
    }
}

/// The `[auth]` table of `auth` as a request file holds it.
fn toml_auth(auth: &Auth) -> String {
    let mut table = toml::Table::new();
    match toml::Value::try_from(auth) {
        Ok(value) => table.insert("auth".to_owned(), value),
        Err(error) => panic!("auth: {error}"),
    };
    match toml::to_string(&table) {
        Ok(text) => text,
        Err(error) => panic!("auth: {error}"),
    }
}
