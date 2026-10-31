//! The `ui_*` tools under the MCP policy (mcp-approvals S5), against the fake bridge, a temp
//! config, a temp audit log and a fake approval window in a temp runtime dir.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use gpui_mcp_protocol::{
    BridgeResult, InputCommand, NodeState, Operation, Rect, Role, UiNode, UiTree, WireRequest,
    WireResponse,
};
use reqflux_engine::approvals::{
    self, ApprovalDecision, ApprovalHandler, ApprovalListener, ApprovalRequest, Reply,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use tempfile::TempDir;

use super::ReqFluxMcp;
use crate::AppId;
use crate::approvals::Approvals;
use crate::bridge::BridgeSession;
use crate::bridge::fake::{
    OperationLog, StartedBridge, pong, recording, result, settled_frame, start_in,
};
use crate::engine_tools::{EngineState, PolicyPaths};
use crate::tools::{ClickArgs, SelectTargetArgs, TextArgs};

const PAUSED: &str =
    "an approval is pending in the window; ui tools are paused until it is answered";

/// A window that answers every approval with `decision`, or holds it unanswered when `None`.
struct Window {
    decision: Option<ApprovalDecision>,
    seen: StdMutex<Vec<ApprovalRequest>>,
    held: StdMutex<Vec<Reply>>,
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
    dir: TempDir,
    endpoints: PathBuf,
    runtime: PathBuf,
    paths: PolicyPaths,
    log: OperationLog,
    bridge: Option<StartedBridge>,
    server: ReqFluxMcp,
}

fn tree() -> UiTree {
    let mut send = UiNode {
        id: "send-button".to_owned(),
        parent: None,
        children: Vec::new(),
        role: Role::Button,
        label: Some("Send".to_owned()),
        description: None,
        bounds: None,
        state: NodeState::default(),
        actions: Vec::new(),
        text: None,
        value: None,
        metadata: BTreeMap::new(),
    };
    send.bounds = Some(Rect {
        x: 10.0,
        y: 10.0,
        width: 80.0,
        height: 30.0,
    });
    let mut nodes = BTreeMap::new();
    nodes.insert(send.id.clone(), send);
    UiTree {
        generation: 1,
        roots: vec!["send-button".to_owned()],
        nodes,
        diagnostics: Vec::new(),
    }
}

fn respond(request: &WireRequest) -> WireResponse {
    match &request.operation {
        Operation::Ping => pong(request),
        Operation::GetTree => result(request, BridgeResult::Tree(tree())),
        Operation::GetPendingFrame => settled_frame(request),
        _ => result(request, BridgeResult::Ack),
    }
}

fn start_bridge(endpoints: &Path, log: &OperationLog, instance: u64) -> StartedBridge {
    start_in(
        endpoints,
        "reqflux",
        instance,
        recording(Arc::clone(log), Arc::new(respond)),
    )
}

/// A server with the config `config` (none when `None`), one fake bridge and approvals asked
/// in a temp runtime dir within `limit`.
fn fixture(config: Option<&str>, limit: Duration) -> Fixture {
    let dir = TempDir::new().unwrap();
    let endpoints = dir.path().join("endpoints");
    std::fs::create_dir_all(&endpoints).unwrap();
    let runtime = dir.path().join("run/reqflux");
    let paths = PolicyPaths {
        config: dir.path().join("config/reqflux/config.toml"),
        audit: dir.path().join("state/reqflux/audit.log"),
    };
    if let Some(text) = config {
        std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
        std::fs::write(&paths.config, text).unwrap();
    }
    let log = OperationLog::default();
    let bridge = start_bridge(&endpoints, &log, 0x01);
    let engine = EngineState::new(
        Arc::new(reqflux_engine::MemorySecretStore::new()),
        Some(dir.path().join("state/reqflux/state.sqlite3")),
        paths.clone(),
    )
    .with_approvals(Approvals::in_dir(Some(runtime.clone()), limit));
    let server = ReqFluxMcp::new(
        BridgeSession::new(endpoints.clone(), AppId::new("reqflux").unwrap()),
        engine,
    );
    Fixture {
        dir,
        endpoints,
        runtime,
        paths,
        log,
        bridge: Some(bridge),
        server,
    }
}

fn window(
    fixture: &Fixture,
    decision: Option<ApprovalDecision>,
) -> (Arc<Window>, ApprovalListener) {
    let window = Arc::new(Window {
        decision,
        seen: StdMutex::new(Vec::new()),
        held: StdMutex::new(Vec::new()),
    });
    let listener = approvals::serve_in(&fixture.runtime, std::process::id(), window.clone())
        .expect("the fake window listens");
    (window, listener)
}

fn text(result: &CallToolResult) -> String {
    match result.content.first() {
        Some(ContentBlock::Text(text)) => text.text.clone(),
        _ => panic!("expected a text block"),
    }
}

fn ok(result: CallToolResult) -> String {
    assert_eq!(result.is_error, Some(false), "{}", text(&result));
    text(&result)
}

fn refused(result: CallToolResult) -> String {
    assert_eq!(result.is_error, Some(true), "{}", text(&result));
    text(&result)
}

fn audit(paths: &PolicyPaths) -> Vec<serde_json::Value> {
    if !paths.audit.exists() {
        return Vec::new();
    }
    reqflux_engine::audit::read(&paths.audit).unwrap()
}

fn inputs(log: &OperationLog) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .filter(|operation| {
            matches!(
                operation,
                Operation::PointerInput { .. } | Operation::Input { .. } | Operation::Focus { .. }
            )
        })
        .count()
}

fn click() -> Parameters<ClickArgs> {
    Parameters(ClickArgs {
        id: "send-button".to_owned(),
        button: None,
        count: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn by_default_tree_reads_run_and_a_click_is_refused_and_audited() {
    let fixture = fixture(None, Duration::from_secs(5));
    let tree = ok(fixture.server.ui_tree().await.unwrap());
    assert!(tree.contains("send-button"));

    let refusal = refused(fixture.server.ui_click(click()).await.unwrap());
    assert!(
        refusal.starts_with("ui_click is an execute tool and execute is \"ask\""),
        "{refusal}"
    );
    assert_eq!(inputs(&fixture.log), 0, "a refused click sends no input");

    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1, "only the click is audited: {lines:?}");
    assert_eq!(lines[0]["tool"], "ui_click");
    assert_eq!(lines[0]["class"], "execute");
    assert_eq!(lines[0]["decision"], "refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_allow_runs_input_and_audits_it_without_the_typed_text() {
    let fixture = fixture(Some("[mcp]\nexecute = \"allow\"\n"), Duration::from_secs(5));
    ok(fixture.server.ui_click(click()).await.unwrap());
    ok(fixture
        .server
        .ui_type(Parameters(TextArgs {
            text: "typed-marker-7f3a".to_owned(),
        }))
        .await
        .unwrap());
    assert!(
        inputs(&fixture.log) >= 3,
        "the click and the text reached the bridge"
    );
    assert!(fixture.log.lock().unwrap().iter().any(|operation| matches!(
        operation,
        Operation::Input {
            command: InputCommand::TypeText { .. }
        }
    )));

    let lines = audit(&fixture.paths);
    let tools: Vec<_> = lines.iter().map(|line| line["tool"].clone()).collect();
    assert_eq!(tools, ["ui_click", "ui_type"]);
    assert!(lines.iter().all(|line| line["decision"] == "allowed"));
    let raw = std::fs::read_to_string(&fixture.paths.audit).unwrap();
    assert!(!raw.contains("typed-marker-7f3a"), "{raw}");
}

#[tokio::test(flavor = "multi_thread")]
async fn read_deny_refuses_tree_reads_unaudited() {
    let fixture = fixture(Some("[mcp]\nread = \"deny\"\n"), Duration::from_secs(5));
    let refusal = refused(fixture.server.ui_tree().await.unwrap());
    assert!(
        refusal.starts_with("ui_tree is a read tool and read is \"deny\""),
        "{refusal}"
    );
    assert!(
        fixture.log.lock().unwrap().is_empty(),
        "the bridge was not asked"
    );
    assert!(audit(&fixture.paths).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn target_tools_stay_unpoliced_under_read_deny() {
    let fixture = fixture(Some("[mcp]\nread = \"deny\"\n"), Duration::from_secs(5));
    let targets = ok(fixture.server.ui_list_targets().await.unwrap());
    assert!(targets.contains("reqflux-0000000000000001"), "{targets}");
    ok(fixture
        .server
        .ui_select_target(Parameters(SelectTargetArgs {
            target_id: "reqflux-0000000000000001".to_owned(),
        }))
        .await
        .unwrap());
    ok(fixture.server.ui_ping().await.unwrap());
    assert!(audit(&fixture.paths).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_allowed_in_the_window_runs_the_click() {
    let fixture = fixture(None, Duration::from_secs(5));
    let (window, _listener) = window(&fixture, Some(ApprovalDecision::AllowOnce));

    ok(fixture.server.ui_click(click()).await.unwrap());
    assert!(inputs(&fixture.log) >= 2, "the click reached the bridge");

    let seen = window.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].tool, "ui_click");
    assert_eq!(seen[0].class, "execute");
    assert_eq!(seen[0].targets, ["reqflux-0000000000000001"]);
    let lines = audit(&fixture.paths);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["decision"], "allowed");
    assert_eq!(lines[0]["reason"], "approved once");
}

#[tokio::test(flavor = "multi_thread")]
async fn allowed_for_the_session_skips_later_asks() {
    let fixture = fixture(None, Duration::from_secs(5));
    let (window, _listener) = window(&fixture, Some(ApprovalDecision::AllowSession));
    ok(fixture.server.ui_click(click()).await.unwrap());
    ok(fixture.server.ui_click(click()).await.unwrap());
    assert_eq!(window.seen.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_bridge_refuses_with_the_pending_message_and_comes_back_on_its_own() {
    let mut fixture = fixture(Some("[mcp]\nexecute = \"allow\"\n"), Duration::from_secs(5));
    let (_window, _listener) = window(&fixture, Some(ApprovalDecision::Deny));
    ok(fixture.server.ui_tree().await.unwrap());

    // The window shuts its bridge down while an approval is pending.
    let paused = fixture.bridge.take().unwrap();
    std::fs::remove_file(&paused.descriptor_path).unwrap();
    drop(paused);
    assert_eq!(refused(fixture.server.ui_tree().await.unwrap()), PAUSED);
    assert_eq!(
        refused(fixture.server.ui_click(click()).await.unwrap()),
        PAUSED
    );

    // The queue empties: the same process reinstalls its bridge with a new instance and token.
    fixture.bridge = Some(start_bridge(&fixture.endpoints, &fixture.log, 0x02));
    ok(fixture.server.ui_tree().await.unwrap());
    ok(fixture.server.ui_click(click()).await.unwrap());
    let targets = ok(fixture.server.ui_list_targets().await.unwrap());
    let targets: serde_json::Value = serde_json::from_str(&targets).unwrap();
    assert_eq!(
        targets["targets"][0]["target_id"],
        "reqflux-0000000000000002"
    );
    assert_eq!(targets["targets"][0]["selected"], true);
    assert!(fixture.dir.path().exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn input_tools_refuse_without_the_bridge_while_this_server_waits_for_its_own_ask() {
    let fixture = fixture(None, Duration::from_secs(5));
    let (window, listener) = window(&fixture, None);
    let server = fixture.server.clone();
    let asking = tokio::spawn(async move { server.ui_click(click()).await.unwrap() });
    for _ in 0..200 {
        if !window.seen.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        window.seen.lock().unwrap().len(),
        1,
        "the click is being asked"
    );

    let typed = tokio::time::timeout(
        Duration::from_secs(2),
        fixture.server.ui_type(Parameters(TextArgs {
            text: "x".to_owned(),
        })),
    )
    .await
    .expect("the refusal does not wait for the ask")
    .unwrap();
    assert_eq!(refused(typed), PAUSED);
    assert_eq!(inputs(&fixture.log), 0, "nothing reached the bridge");

    window.held.lock().unwrap().clear();
    drop(listener);
    let click = refused(asking.await.unwrap());
    assert!(click.contains("no answer"), "{click}");
    assert_eq!(inputs(&fixture.log), 0);
}
