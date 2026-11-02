//! `cookie_list`, `cookie_delete` and `cookie_clear` (v1-features S5): the open workspace's
//! cookie jar, kept in a temp app state, with every cookie value masked. Sends go through the
//! `[network]` of a temp config file.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::path::Path;
use std::sync::Arc;

use reqflux_engine::policy::ToolClass;
use reqflux_engine::{MASK, MemorySecretStore};
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, CookieArgs, EngineState, ExecuteArgs, PathArgs, RequestCreateArgs,
    WorkspaceCreateArgs,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn state(dir: &TempDir, config: &str) -> EngineState {
    EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(dir.path().join("state.sqlite3")),
        common::paths(dir.path(), Some(config)),
    )
}

fn ok(result: Result<String, reqflux_mcp::tools::ToolError>) -> Value {
    match result {
        Ok(text) => serde_json::from_str(&text).unwrap(),
        Err(error) => panic!("tool failed: {error}"),
    }
}

/// Creates the workspace `<dir>/ws` with one GET-less request `R` to `url`; returns its id.
fn workspace_with_request(state: &mut EngineState, dir: &Path, url: &str) -> String {
    ok(engine_tools::workspace_create(
        state,
        WorkspaceCreateArgs {
            path: dir.join("ws").display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    let api = ok(engine_tools::collection_create(
        state,
        CollectionCreateArgs {
            name: Some("Api".to_owned()),
        },
    ));
    let request = ok(engine_tools::request_create(
        state,
        RequestCreateArgs {
            parent_id: api["id"].as_str().unwrap().to_owned(),
            name: Some("R".to_owned()),
            method: Some("get".to_owned()),
            url: Some(url.to_owned()),
        },
    ));
    request["id"].as_str().unwrap().to_owned()
}

fn execute(state: &mut EngineState, id: &str) -> Value {
    ok(engine_tools::request_execute(
        state,
        ExecuteArgs {
            id: id.to_owned(),
            environment: None,
            vars: None,
        },
    ))
}

fn login_server(runtime: &tokio::runtime::Runtime) -> MockServer {
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(path("/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "session=s3cr3t-cookie; Path=/; HttpOnly"),
            )
            .mount(&server)
            .await;
        server
    })
}

#[test]
fn the_jar_is_listed_masked_kept_across_sessions_deleted_and_cleared() {
    let dir = TempDir::new().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = login_server(&runtime);
    let mut state = state(&dir, common::ALLOW_ALL);
    let id = workspace_with_request(&mut state, dir.path(), &format!("{}/login", server.uri()));

    let executed = execute(&mut state, &id);
    let listed = ok(engine_tools::cookie_list(&mut state));

    let set_cookie = executed["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == "set-cookie")
        .unwrap()
        .clone();
    assert_eq!(set_cookie[1], format!("session={MASK}; Path=/; HttpOnly"));
    assert_eq!(
        listed,
        json!({ "cookies": [{
            "domain": "127.0.0.1",
            "host_only": true,
            "path": "/",
            "name": "session",
            "value": MASK,
            "expires": null,
            "secure": false,
            "http_only": true,
            "same_site": null,
        }]})
    );
    assert!(!listed.to_string().contains("s3cr3t-cookie"));

    // A new session on the same app state finds the jar.
    let mut next = self::state(&dir, common::ALLOW_ALL);
    ok(engine_tools::workspace_open(
        &mut next,
        PathArgs {
            path: dir.path().join("ws").display().to_string(),
        },
    ));
    assert_eq!(ok(engine_tools::cookie_list(&mut next)), listed);

    let deleted = ok(engine_tools::cookie_delete(
        &mut next,
        CookieArgs {
            domain: "127.0.0.1".to_owned(),
            path: "/".to_owned(),
            name: "session".to_owned(),
        },
    ));
    assert_eq!(deleted, json!({ "deleted": true }));
    assert_eq!(
        ok(engine_tools::cookie_list(&mut next)),
        json!({ "cookies": [] })
    );
    execute(&mut next, &id);
    assert_eq!(
        ok(engine_tools::cookie_clear(&mut next)),
        json!({ "cleared": 1 })
    );
    assert_eq!(
        ok(engine_tools::cookie_list(&mut next)),
        json!({ "cookies": [] })
    );
}

#[test]
fn deleting_and_clearing_cookies_are_destructive() {
    let dir = TempDir::new().unwrap();
    let mut state = state(&dir, "[mcp]\nwrite = \"allow\"\ndestructive = \"deny\"\n");
    workspace_with_request(&mut state, dir.path(), "http://127.0.0.1:9/");

    let delete = engine_tools::cookie_delete(
        &mut state,
        CookieArgs {
            domain: "a".to_owned(),
            path: "/".to_owned(),
            name: "b".to_owned(),
        },
    );
    let clear = engine_tools::cookie_clear(&mut state);

    assert!(delete.is_err(), "{delete:?}");
    assert!(clear.is_err(), "{clear:?}");
    assert_eq!(
        ok(engine_tools::cookie_list(&mut state)),
        json!({ "cookies": [] })
    );
    assert_eq!(
        engine_tools::tool_class("cookie_list"),
        Some(ToolClass::Read)
    );
    assert_eq!(
        engine_tools::tool_class("cookie_clear"),
        Some(ToolClass::Destructive)
    );
}

#[test]
fn a_send_goes_through_the_proxy_of_the_config_file() {
    let dir = TempDir::new().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let proxy = runtime.block_on(async {
        let proxy = MockServer::start().await;
        Mock::given(path("/via"))
            .and(header("host", "target.invalid"))
            .respond_with(ResponseTemplate::new(200).set_body_string("proxied"))
            .mount(&proxy)
            .await;
        proxy
    });
    let config = format!(
        "{}\n[network.proxy]\nmode = \"manual\"\nurl = \"{}\"\n",
        common::ALLOW_ALL,
        proxy.uri()
    );
    let mut state = state(&dir, &config);
    let id = workspace_with_request(&mut state, dir.path(), "http://target.invalid/via");

    let executed = execute(&mut state, &id);

    assert_eq!(executed["status"], 200);
    assert_eq!(executed["body"], "proxied");
}
