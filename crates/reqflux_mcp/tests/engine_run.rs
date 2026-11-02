//! `collection_run` (v1-features S1) under the MCP policy: on temp workspaces with a temp
//! config file, a temp audit log, a temp app state, a temp approvals runtime dir and an
//! in-memory secret store.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqflux_engine::approvals::{
    self, ApprovalDecision, ApprovalHandler, ApprovalListener, ApprovalRequest, Reply,
};
use reqflux_engine::{MASK, MemorySecretStore};
use reqflux_mcp::approvals::Approvals;
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, EngineState, EnvironmentCreateArgs, HistoryListArgs, PolicyPaths,
    RequestCreateArgs, RunArgs, VariableSetArgs, WorkspaceCreateArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "sk-live-0123456789";

fn ok(result: Result<String, ToolError>) -> Value {
    match result {
        Ok(text) => serde_json::from_str(&text).unwrap(),
        Err(error) => panic!("tool failed: {error}"),
    }
}

fn err(result: Result<String, ToolError>) -> String {
    match result {
        Ok(text) => panic!("expected a refusal, got {text}"),
        Err(error) => error.to_string(),
    }
}

fn audit(paths: &PolicyPaths) -> Vec<Value> {
    reqflux_engine::audit::read(&paths.audit).unwrap_or_default()
}

fn audit_text(paths: &PolicyPaths) -> String {
    std::fs::read_to_string(&paths.audit).unwrap_or_default()
}

/// A workspace `Demo` with collection `API` holding `One` (to `a`, with the secret global
/// `token` in its query), `Two` (to `b`) and `Three` (to `a`), environments `dev` and
/// `prod`, built under an allow-all config that is then replaced by `config`, and the audit
/// log cleared.
struct Fixture {
    _dir: TempDir,
    runtime: PathBuf,
    state: EngineState,
    paths: PolicyPaths,
    collection: String,
    requests: Vec<String>,
}

fn fixture(a: &str, b: &str, config: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let paths = common::allow_all(dir.path());
    let runtime = dir.path().join("run/reqflux");
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(dir.path().join("state/reqflux/state.sqlite3")),
        paths.clone(),
    )
    .with_approvals(Approvals::in_dir(
        Some(runtime.clone()),
        Duration::from_secs(5),
    ));
    ok(engine_tools::workspace_create(
        &mut state,
        WorkspaceCreateArgs {
            path: dir.path().join("ws").display().to_string(),
            name: "Demo".to_owned(),
        },
    ));
    let collection = ok(engine_tools::collection_create(
        &mut state,
        CollectionCreateArgs {
            name: Some("API".to_owned()),
        },
    ))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut requests = Vec::new();
    for (name, url) in [
        ("One", format!("{a}/one?key={{{{token}}}}")),
        ("Two", format!("{b}/two")),
        ("Three", format!("{a}/three")),
    ] {
        let created = ok(engine_tools::request_create(
            &mut state,
            RequestCreateArgs {
                parent_id: collection.clone(),
                name: Some(name.to_owned()),
                method: Some("GET".to_owned()),
                url: Some(url),
            },
        ));
        requests.push(created["id"].as_str().unwrap().to_owned());
    }
    ok(engine_tools::variable_set(
        &mut state,
        VariableSetArgs {
            scope: "global".to_owned(),
            key: "token".to_owned(),
            value: Some(SECRET.to_owned()),
            secret: Some(true),
            enabled: None,
            description: None,
        },
    ));
    for name in ["dev", "prod"] {
        ok(engine_tools::environment_create(
            &mut state,
            EnvironmentCreateArgs {
                name: Some(name.to_owned()),
            },
        ));
    }
    common::write_config(&paths, config);
    std::fs::remove_file(&paths.audit).unwrap();
    Fixture {
        _dir: dir,
        runtime,
        state,
        paths,
        collection,
        requests,
    }
}

fn run(
    fixture: &mut Fixture,
    target: &str,
    environment: Option<&str>,
    bail: bool,
) -> Result<String, ToolError> {
    engine_tools::collection_run(
        &mut fixture.state,
        RunArgs {
            target: target.to_owned(),
            environment: environment.map(str::to_owned),
            bail: Some(bail),
        },
    )
}

fn history(fixture: &mut Fixture) -> Vec<Value> {
    ok(engine_tools::history_list(
        &mut fixture.state,
        HistoryListArgs { limit: None },
    ))["entries"]
        .as_array()
        .unwrap()
        .clone()
}

async fn mock_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn an_allowed_run_returns_the_masked_summary_audits_each_request_and_records_history() {
    let a = mock_server().await;
    let b = mock_server().await;
    let mut fixture =
        tokio::task::block_in_place(|| fixture(&a.uri(), &b.uri(), "[mcp]\nexecute = \"allow\"\n"));

    // By slug path.
    let text = tokio::task::block_in_place(|| run(&mut fixture, "api", None, false)).unwrap();
    assert!(!text.contains(SECRET), "{text}");
    let output: Value = serde_json::from_str(&text).unwrap();
    let results = output["results"].as_array().unwrap();
    assert_eq!(results.len(), 3, "{output}");
    let names: Vec<&str> = results
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["One", "Two", "Three"]);
    let first = &results[0];
    assert_eq!(first["method"], "GET");
    assert_eq!(first["url"], format!("{}/one?key={MASK}", a.uri()));
    assert_eq!(first["status"], 200);
    assert!(first["elapsed_ms"].is_u64());
    assert_eq!(first["tests"], serde_json::json!([]));
    assert!(first["error"].is_null());
    assert_eq!(output["totals"]["requests"], 3);
    assert_eq!(output["totals"]["failed_requests"], 0);
    assert_eq!(output["totals"]["tests_passed"], 0);
    assert_eq!(output["totals"]["tests_failed"], 0);
    assert_eq!(output["passed"], true);
    assert_eq!(output["end"], "completed");
    assert!(output["elapsed_ms"].is_u64());
    assert_eq!(a.received_requests().await.unwrap().len(), 2);
    assert_eq!(b.received_requests().await.unwrap().len(), 1);

    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0]["tool"], "collection_run");
    assert_eq!(lines[0]["class"], "execute");
    assert_eq!(lines[0]["decision"], "allowed");
    assert_eq!(lines[0]["targets"][0], fixture.collection.as_str());
    assert!(lines[0].get("url").is_none(), "{}", lines[0]);
    for (line, request) in lines[1..].iter().zip(&fixture.requests) {
        assert_eq!(line["tool"], "collection_run");
        assert_eq!(line["decision"], "allowed");
        assert_eq!(line["method"], "GET");
        assert_eq!(line["targets"][0], request.as_str());
    }
    assert_eq!(lines[1]["url"], format!("{}/one?key={MASK}", a.uri()));
    assert!(!audit_text(&fixture.paths).contains(SECRET));

    let entries = tokio::task::block_in_place(|| history(&mut fixture));
    assert_eq!(entries.len(), 3, "{entries:?}");
    assert!(!format!("{entries:?}").contains(SECRET));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_outside_hosts_is_refused_mid_run_and_the_run_goes_on() {
    let a = mock_server().await;
    let b = mock_server().await;
    let config = format!(
        "[mcp]\nexecute = \"allow\"\nhosts = [\"127.0.0.1:{}\"]\n",
        a.address().port()
    );
    let mut fixture = tokio::task::block_in_place(|| fixture(&a.uri(), &b.uri(), &config));
    let collection = fixture.collection.clone();

    let output = tokio::task::block_in_place(|| ok(run(&mut fixture, &collection, None, false)));
    let refusal = format!(
        "collection_run may not reach 127.0.0.1:{}: it is not in hosts = [\"127.0.0.1:{}\"] in {} ([mcp] hosts)",
        b.address().port(),
        a.address().port(),
        fixture.paths.config.display()
    );
    let results = output["results"].as_array().unwrap();
    assert_eq!(results.len(), 3, "{output}");
    assert_eq!(results[1]["status"], Value::Null);
    assert_eq!(results[1]["error"], refusal.as_str());
    assert_eq!(results[1]["url"], format!("{}/two", b.uri()));
    assert_eq!(results[2]["status"], 200);
    assert_eq!(output["totals"]["failed_requests"], 1);
    assert_eq!(output["passed"], false);
    assert_eq!(output["end"], "completed");
    assert!(b.received_requests().await.unwrap().is_empty());
    assert_eq!(a.received_requests().await.unwrap().len(), 2);

    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[1]["decision"], "allowed");
    assert_eq!(lines[2]["decision"], "refused");
    assert_eq!(lines[2]["reason"], refusal.as_str());
    assert_eq!(lines[2]["url"], format!("{}/two", b.uri()));
    assert_eq!(lines[3]["decision"], "allowed");

    // A refused request sends nothing, so history has only the two sends.
    assert_eq!(
        tokio::task::block_in_place(|| history(&mut fixture)).len(),
        2
    );

    // With bail, the refusal ends the run.
    let output = tokio::task::block_in_place(|| ok(run(&mut fixture, &collection, None, true)));
    assert_eq!(output["end"], "bailed");
    assert_eq!(output["results"].as_array().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_refuses_the_run_sends_nothing_and_audits_it() {
    let a = mock_server().await;
    let mut fixture =
        tokio::task::block_in_place(|| fixture(&a.uri(), &a.uri(), "[mcp]\nexecute = \"deny\"\n"));
    let collection = fixture.collection.clone();
    let refused = err(tokio::task::block_in_place(|| {
        run(&mut fixture, &collection, None, false)
    }));
    assert_eq!(
        refused,
        format!(
            "collection_run is an execute tool and execute is \"deny\" in {} ([mcp] execute)",
            fixture.paths.config.display()
        )
    );
    assert!(a.received_requests().await.unwrap().is_empty());
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "refused");
    assert_eq!(lines[0]["targets"][0], collection.as_str());
    assert!(tokio::task::block_in_place(|| history(&mut fixture)).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_environment_outside_environments_is_refused() {
    let a = mock_server().await;
    let mut fixture = tokio::task::block_in_place(|| {
        fixture(
            &a.uri(),
            &a.uri(),
            "[mcp]\nexecute = \"allow\"\nenvironments = [\"dev\"]\n",
        )
    });
    let collection = fixture.collection.clone();
    let refused = err(tokio::task::block_in_place(|| {
        run(&mut fixture, &collection, Some("prod"), false)
    }));
    assert_eq!(
        refused,
        format!(
            "collection_run may not use the environment prod: it is not in environments = [\"dev\"] in {} ([mcp] environments)",
            fixture.paths.config.display()
        )
    );
    assert!(a.received_requests().await.unwrap().is_empty());
    ok(tokio::task::block_in_place(|| {
        run(&mut fixture, &collection, Some("dev"), false)
    }));
}

/// A window that answers every request with `decision`.
struct Window {
    decision: ApprovalDecision,
    seen: Mutex<Vec<ApprovalRequest>>,
}

impl ApprovalHandler for Window {
    fn request(&self, request: ApprovalRequest, reply: Reply) {
        self.seen.lock().unwrap().push(request);
        assert!(reply.send(self.decision));
    }
}

fn window(fixture: &Fixture, decision: ApprovalDecision) -> (Arc<Window>, ApprovalListener) {
    let window = Arc::new(Window {
        decision,
        seen: Mutex::new(Vec::new()),
    });
    let listener = approvals::serve_in(&fixture.runtime, std::process::id(), window.clone())
        .expect("the fake window listens");
    (window, listener)
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_without_a_window_refuses_with_the_policy_message() {
    let a = mock_server().await;
    let mut fixture =
        tokio::task::block_in_place(|| fixture(&a.uri(), &a.uri(), "[mcp]\nexecute = \"ask\"\n"));
    let collection = fixture.collection.clone();
    let refused = err(tokio::task::block_in_place(|| {
        run(&mut fixture, &collection, None, false)
    }));
    assert!(
        refused.starts_with(&format!(
            "collection_run is an execute tool and execute is \"ask\" in {} ([mcp] execute)",
            fixture.paths.config.display()
        )),
        "{refused}"
    );
    assert!(a.received_requests().await.unwrap().is_empty());
    assert_eq!(audit(&fixture.paths)[0]["decision"], "refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_names_the_target_and_request_count_and_runs_once_allowed() {
    let a = mock_server().await;
    let mut fixture =
        tokio::task::block_in_place(|| fixture(&a.uri(), &a.uri(), "[mcp]\nexecute = \"ask\"\n"));
    let (window, listener) = window(&fixture, ApprovalDecision::AllowOnce);
    let collection = fixture.collection.clone();
    let output = tokio::task::block_in_place(|| ok(run(&mut fixture, &collection, None, false)));
    assert_eq!(output["totals"]["requests"], 3);
    let seen = window.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].tool, "collection_run");
    assert_eq!(seen[0].class, "execute");
    assert_eq!(seen[0].targets, ["API · 3 requests"]);
    assert_eq!(seen[0].item_ids, std::slice::from_ref(&collection));
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0]["reason"], "approved once");

    // A denial refuses the next run before anything is sent.
    drop(listener);
    let deny = Arc::new(Window {
        decision: ApprovalDecision::Deny,
        seen: Mutex::new(Vec::new()),
    });
    let _listener = approvals::serve_in(&fixture.runtime, std::process::id(), deny).unwrap();
    let before = a.received_requests().await.unwrap().len();
    let refused = err(tokio::task::block_in_place(|| {
        run(&mut fixture, &collection, None, false)
    }));
    assert!(refused.ends_with("; denied in the window"), "{refused}");
    assert_eq!(a.received_requests().await.unwrap().len(), before);
}
