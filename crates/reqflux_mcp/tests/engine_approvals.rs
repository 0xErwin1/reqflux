//! In-app approvals of `ask` calls (mcp-approvals S1, S2, S6), against a fake window
//! listening in a temp runtime dir.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqflux_engine::MemorySecretStore;
use reqflux_engine::approvals::{
    self, ApprovalDecision, ApprovalHandler, ApprovalListener, ApprovalRequest, Reply,
    WindowDescriptor,
};
use reqflux_mcp::approvals::Approvals;
use reqflux_mcp::engine_tools::{
    self, CollectionCreateArgs, EngineState, ExecuteArgs, PolicyPaths, RequestCreateArgs,
    WorkspaceCreateArgs,
};
use reqflux_mcp::tools::ToolError;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const ASK: &str =
    "[mcp]\nread = \"allow\"\nwrite = \"ask\"\nexecute = \"ask\"\ndestructive = \"ask\"\n";

/// A window that answers every request with `decision`, or holds it unanswered when `None`.
struct Window {
    decision: Option<ApprovalDecision>,
    seen: Mutex<Vec<ApprovalRequest>>,
    held: Mutex<Vec<Reply>>,
}

impl ApprovalHandler for Window {
    fn request(&self, request: ApprovalRequest, reply: Reply) {
        self.seen.lock().unwrap().push(request);
        match self.decision {
            Some(decision) => assert!(reply.send(decision)),
            None => self.held.lock().unwrap().push(reply),
        }
    }
}

struct Fixture {
    _dir: TempDir,
    runtime: std::path::PathBuf,
    state: EngineState,
    paths: PolicyPaths,
    workspace_id: String,
    collection: String,
    request: String,
}

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

/// A workspace `Demo` with collection `API` and request `Ping` to `url`, built under an
/// allow-all config that is then replaced by [`ASK`], approvals asked in a temp runtime dir
/// with `limit`, and the audit log cleared.
fn fixture(url: &str, limit: Duration) -> Fixture {
    let dir = TempDir::new().unwrap();
    let paths = common::allow_all(dir.path());
    let runtime = dir.path().join("run/reqflux");
    let mut state = EngineState::new(
        Arc::new(MemorySecretStore::new()),
        Some(dir.path().join("state/reqflux/state.sqlite3")),
        paths.clone(),
    )
    .with_approvals(Approvals::in_dir(Some(runtime.clone()), limit));
    let created = ok(engine_tools::workspace_create(
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
    let request = ok(engine_tools::request_create(
        &mut state,
        RequestCreateArgs {
            parent_id: collection.clone(),
            name: Some("Ping".to_owned()),
            method: Some("GET".to_owned()),
            url: Some(url.to_owned()),
        },
    ))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    common::write_config(&paths, ASK);
    std::fs::remove_file(&paths.audit).unwrap();
    Fixture {
        workspace_id: created["id"].as_str().unwrap().to_owned(),
        _dir: dir,
        runtime,
        state,
        paths,
        collection,
        request,
    }
}

fn window(
    fixture: &Fixture,
    decision: Option<ApprovalDecision>,
) -> (Arc<Window>, ApprovalListener) {
    let window = Arc::new(Window {
        decision,
        seen: Mutex::new(Vec::new()),
        held: Mutex::new(Vec::new()),
    });
    let listener = approvals::serve_in(&fixture.runtime, std::process::id(), window.clone())
        .expect("the fake window listens");
    (window, listener)
}

fn execute(fixture: &mut Fixture) -> Result<String, ToolError> {
    engine_tools::request_execute(
        &mut fixture.state,
        ExecuteArgs {
            id: fixture.request.clone(),
            environment: None,
            vars: None,
        },
    )
}

fn create_folder(fixture: &mut Fixture) -> Result<String, ToolError> {
    create_folder_named(fixture, "secret-folder-name-arg")
}

fn create_folder_named(fixture: &mut Fixture, name: &str) -> Result<String, ToolError> {
    engine_tools::folder_create(
        &mut fixture.state,
        engine_tools::FolderCreateArgs {
            parent_id: fixture.collection.clone(),
            name: Some(name.to_owned()),
        },
    )
}

fn audit(paths: &PolicyPaths) -> Vec<Value> {
    reqflux_engine::audit::read(&paths.audit).unwrap()
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
async fn allow_once_runs_the_execute_and_audits_approved_once() {
    let server = mock_server().await;
    let url = format!("{}/ping?q=1", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, Duration::from_secs(5)));
    let (window, _listener) = window(&fixture, Some(ApprovalDecision::AllowOnce));

    let result = tokio::task::block_in_place(|| ok(execute(&mut fixture)));
    assert_eq!(result["status"], 200);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    let seen = window.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let request = &seen[0];
    assert_eq!(request.tool, "request_execute");
    assert_eq!(request.class, "execute");
    let workspace = request.workspace.as_ref().unwrap();
    assert_eq!(workspace.id, fixture.workspace_id);
    assert_eq!(workspace.name, "Demo");
    assert_eq!(request.targets, vec!["Ping".to_owned()]);
    assert_eq!(request.method.as_deref(), Some("GET"));
    assert_eq!(request.url.as_deref(), Some(url.as_str()));

    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["decision"], "allowed");
    assert_eq!(lines[0]["reason"], "approved once");
    assert_eq!(lines[0]["method"], "GET");

    // Allow once does not carry over: the next call asks again.
    tokio::task::block_in_place(|| ok(execute(&mut fixture)));
    assert_eq!(window.seen.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_refuses_the_execute_sends_nothing_and_audits_it() {
    let server = mock_server().await;
    let url = format!("{}/ping", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, Duration::from_secs(5)));
    let (_window, _listener) = window(&fixture, Some(ApprovalDecision::Deny));

    let refused = tokio::task::block_in_place(|| err(execute(&mut fixture)));
    assert!(refused.contains("denied in the window"), "{refused}");
    assert!(server.received_requests().await.unwrap().is_empty());
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["decision"], "refused");
    assert_eq!(lines[0]["reason"].as_str().unwrap(), refused);
}

/// A window that denies every request with `reason`.
struct Refuser(String);

impl ApprovalHandler for Refuser {
    fn request(&self, _request: ApprovalRequest, reply: Reply) {
        assert!(reply.answer(ApprovalDecision::Deny, Some(self.0.clone())));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_denial_reason_reaches_the_refusal_and_the_audit_line_cleaned() {
    let server = mock_server().await;
    let url = format!("{}/ping", server.uri());
    let mut fixture = tokio::task::block_in_place(|| fixture(&url, Duration::from_secs(5)));
    let reason = format!("  not on\nprod\u{7}{}  ", "x".repeat(300));
    let _listener = approvals::serve_in(
        &fixture.runtime,
        std::process::id(),
        Arc::new(Refuser(reason)),
    )
    .expect("the fake window listens");

    let refused = tokio::task::block_in_place(|| err(execute(&mut fixture)));
    let expected = format!("denied in the window: not onprod{}", "x".repeat(190));
    assert!(refused.ends_with(&expected), "{refused}");
    assert!(server.received_requests().await.unwrap().is_empty());
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["decision"], "refused");
    assert_eq!(lines[0]["reason"].as_str().unwrap(), refused);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blank_denial_reason_is_left_out() {
    let mut fixture =
        tokio::task::block_in_place(|| fixture("http://127.0.0.1:9/x", Duration::from_secs(5)));
    let _listener = approvals::serve_in(
        &fixture.runtime,
        std::process::id(),
        Arc::new(Refuser(" \n ".to_owned())),
    )
    .expect("the fake window listens");
    let refused = tokio::task::block_in_place(|| err(execute(&mut fixture)));
    assert!(refused.ends_with("; denied in the window"), "{refused}");
}

#[test]
fn allow_session_is_cached_for_the_class_and_workspace() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Duration::from_secs(5));
    let (window, _listener) = window(&fixture, Some(ApprovalDecision::AllowSession));

    ok(create_folder(&mut fixture));
    ok(create_folder_named(&mut fixture, "second-folder-name-arg"));
    let seen = window.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "the second write is not asked");
    assert_eq!(seen[0].tool, "folder_create");
    assert_eq!(seen[0].class, "write");
    assert_eq!(seen[0].targets, vec!["API".to_owned()]);
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 2, "{lines:?}");
    for line in &lines {
        assert_eq!(line["decision"], "allowed");
        assert_eq!(line["reason"], "approved for this session");
    }
    // No argument value reaches the window or the log.
    assert!(!format!("{seen:?}").contains("secret-folder-name-arg"));
    let text = std::fs::read_to_string(&fixture.paths.audit).unwrap();
    assert!(!text.contains("secret-folder-name-arg"));
    assert!(!text.contains("second-folder-name-arg"));

    // Another class is asked again.
    engine_tools::item_delete(
        &mut fixture.state,
        engine_tools::ItemArgs {
            id: fixture.request.clone(),
        },
    )
    .unwrap();
    assert_eq!(window.seen.lock().unwrap().len(), 2);
}

#[test]
fn no_answer_within_the_limit_refuses() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Duration::from_millis(300));
    let (window, _listener) = window(&fixture, None);

    let refused = err(create_folder(&mut fixture));
    assert!(
        refused.contains("no answer from the window within"),
        "{refused}"
    );
    assert_eq!(window.seen.lock().unwrap().len(), 1);
    let lines = audit(&fixture.paths);
    assert_eq!(lines[0]["decision"], "refused");
    assert!(lines[0]["reason"].as_str().unwrap().contains("no answer"));
}

#[test]
fn without_a_window_ask_refuses_with_the_policy_message() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Duration::from_secs(5));
    let refused = err(create_folder(&mut fixture));
    let policy = format!(
        "folder_create is a write tool and write is \"ask\" in {} ([mcp] write); ask needs an approval in the ReqFlux \
         window",
        fixture.paths.config.display()
    );
    assert!(refused.starts_with(&policy), "{refused}");
    assert!(refused.contains("no window is running"), "{refused}");
    let lines = audit(&fixture.paths);
    assert_eq!(lines[0]["decision"], "refused");
    assert!(lines[0]["reason"].as_str().unwrap().contains("no window"));
}

/// A window whose answer is not a response: the call is refused as no answer.
#[test]
fn a_malformed_reply_refuses() {
    let mut fixture = fixture("http://127.0.0.1:9/x", Duration::from_secs(5));
    let socket = raw_window(&fixture.runtime, b"not json\n");
    let refused = err(create_folder(&mut fixture));
    assert!(refused.contains("no answer from the window"), "{refused}");
    drop(socket);
}

/// A listener in `dir` advertised for this process that reads one line and writes `reply`.
fn raw_window(dir: &Path, reply: &'static [u8]) -> std::thread::JoinHandle<()> {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let pid = std::process::id();
    let socket = approvals::socket_path(dir, pid);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let descriptor = approvals::descriptor_path(dir, pid);
    let text = serde_json::to_string(&WindowDescriptor {
        version: approvals::PROTOCOL_VERSION,
        pid,
        socket,
    })
    .unwrap();
    std::fs::write(&descriptor, text).unwrap();
    std::fs::set_permissions(&descriptor, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        stream.write_all(reply).unwrap();
    })
}
