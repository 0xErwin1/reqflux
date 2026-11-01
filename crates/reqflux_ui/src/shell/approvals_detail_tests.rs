//! The MCP approvals view as the artboard draws it (mcp-approvals S3, T6): the header, the
//! rows, the detail read from the window's own workspace with its secrets masked, the
//! fallback for a call on another workspace, and the `a`, `d` and `s` keys.

use gpui::{Entity, TestAppContext, VisualTestContext};
use reqflux_core::model::Id;
use reqflux_engine::approvals::{ApprovalDecision, ApprovalRequest, WorkspaceRef};
use reqflux_engine::{SecretKey, SecretScope};
use tempfile::TempDir;

use super::Shell;
use super::approvals::POLL;
use super::approvals_tests::{answer, arrive, decision, label, listen, pending};
use super::draft_tests::{bounds, click, present, render, state_file};
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::{demo_workspace, write};

const WORKSPACE: &str = "01J00000000000000000000900";
const COLLECTION: &str = "01J00000000000000000000011";
const FOLDER: &str = "01J00000000000000000000012";
const CREATE: &str = "01J00000000000000000000013";
const PROD: &str = "01J00000000000000000000016";
const SECRET: &str = "tok-SUPERSECRET-123";

/// `Demo` with collection `Billing API` (`base_url`, secret `access_token`) and folder
/// `invoices` holding `POST Create invoice`: bearer auth and an `X-Api-Key` header from the
/// secret, `X-Tenant-Id` from the environment `prod`, and a JSON body.
fn billing_workspace() -> TempDir {
    let dir = demo_workspace();
    let root = dir.path();
    write(
        root,
        "collections/billing/collection.toml",
        &format!(
            "id = \"{COLLECTION}\"\nname = \"Billing API\"\n\n\
             [[variables]]\nkey = \"base_url\"\nvalue = \"https://api.test\"\n\n\
             [[variables]]\nkey = \"access_token\"\nsecret = true\n"
        ),
    );
    write(
        root,
        "collections/billing/invoices/folder.toml",
        &format!("id = \"{FOLDER}\"\nname = \"invoices\"\n"),
    );
    write(
        root,
        "collections/billing/invoices/create.req.toml",
        &format!(
            "id = \"{CREATE}\"\nname = \"Create invoice\"\nmethod = \"POST\"\n\
             url = \"{{{{base_url}}}}/v1/invoices\"\n\n\
             [[headers]]\nkey = \"X-Api-Key\"\nvalue = \"{{{{access_token}}}}\"\n\n\
             [[headers]]\nkey = \"X-Tenant-Id\"\nvalue = \"{{{{tenant_id}}}}\"\n\n\
             [auth]\ntype = \"bearer\"\ntoken = \"{{{{access_token}}}}\"\n\n\
             [body]\nmode = \"raw\"\nlanguage = \"json\"\ntext = \"{{\\\"currency\\\": \\\"eur\\\"}}\"\n"
        ),
    );
    write(
        root,
        "environments/prod.env.toml",
        &format!(
            "id = \"{PROD}\"\nname = \"prod\"\n\n\
             [[variables]]\nkey = \"tenant_id\"\nvalue = \"tn_prod_4821\"\n"
        ),
    );
    dir
}

/// A shell on [`billing_workspace`] with the secret `access_token` in its store, listening
/// for approvals, with the automation tree attached.
fn window(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    gpui_mcp::Automation,
    Vec<TempDir>,
) {
    let dir = billing_workspace();
    let (state_dir, state_path) = state_file();
    let (shell, cx) = render(cx, &dir, &state_path);
    cx.update(|_, cx| {
        let OpenedWorkspace::Loaded(engine) = &shell.read(cx).workspace else {
            panic!("the workspace did not load");
        };
        let key = SecretKey {
            workspace_id: WORKSPACE
                .parse::<Id>()
                .unwrap_or_else(|error| panic!("{error}")),
            scope: SecretScope::Collection(
                COLLECTION
                    .parse::<Id>()
                    .unwrap_or_else(|error| panic!("{error}")),
            ),
            key: "access_token".to_owned(),
        };
        if let Err(error) = engine.secret_store().set(&key, SECRET) {
            panic!("store the secret: {error}");
        }
    });
    let runtime = listen(cx, &shell);
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    (shell, cx, automation, vec![dir, state_dir, runtime])
}

/// A call of `tool` on [`CREATE`] in `prod` from Claude Code, the way `reqflux mcp` sends it.
fn call(id: u64, tool: &str) -> ApprovalRequest {
    let execute = tool == "request_execute";
    ApprovalRequest {
        version: reqflux_engine::approvals::PROTOCOL_VERSION,
        id,
        tool: tool.to_owned(),
        class: if execute { "execute" } else { "write" }.to_owned(),
        workspace: Some(WorkspaceRef {
            id: WORKSPACE.to_owned(),
            name: "Demo".to_owned(),
        }),
        targets: vec!["Create invoice".to_owned()],
        method: execute.then(|| "POST".to_owned()),
        url: execute.then(|| "https://api.test/v1/invoices".to_owned()),
        client: execute.then(|| "Claude Code".to_owned()),
        item_ids: vec![CREATE.to_owned()],
        environment: execute.then(|| "prod".to_owned()),
        policy: Some(format!(
            "{} → approval",
            if execute { "execute" } else { "write" }
        )),
        requested_at: Some(1),
    }
}

/// Every label of the automation tree with `prefix`, in id order.
fn labels(automation: &gpui_mcp::Automation, prefix: &str) -> Vec<String> {
    let snapshot = automation.snapshot();
    let mut found: Vec<(String, String)> = snapshot
        .nodes
        .iter()
        .filter(|(id, _)| id.starts_with(prefix))
        .filter_map(|(id, node)| node.label.clone().map(|label| (id.clone(), label)))
        .collect();
    found.sort();
    found.into_iter().map(|(_, label)| label).collect()
}

#[gpui::test]
fn the_view_keeps_the_sidebar_and_shows_the_header_and_the_rows(cx: &mut TestAppContext) {
    let (shell, cx, automation, _dirs) = window(cx);
    let first = arrive(cx, &shell, call(1, "request_execute"), 1);
    let second = arrive(cx, &shell, call(2, "request_update"), 2);
    click(cx, "rail-approvals");
    present(cx, "sidebar");
    present(cx, "approvals-view");
    assert_eq!(
        label(&automation, "doc-tab-approvals").as_deref(),
        Some("MCP approvals")
    );
    for (id, text) in [
        ("approvals-title", "MCP approvals"),
        (
            "approvals-subtitle",
            "agent calls that a policy parked for a person",
        ),
        ("approvals-refresh", "Refresh"),
        ("approvals-pending-label", "Pending · 2"),
        ("approvals-row-1-class", "Execute"),
        ("approvals-row-1-request", "POST /v1/invoices · prod"),
        ("approvals-row-1-meta", "Claude Code · waiting just now"),
        ("approvals-row-2-class", "Write"),
        (
            "approvals-row-2-request",
            "Billing API › invoices › Create invoice",
        ),
        ("approvals-row-2-meta", "an agent · waiting just now"),
        (
            "approvals-footer",
            "j k move · decisions go to the audit log",
        ),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(text), "{id}");
    }
    click(cx, "approvals-refresh");
    assert_eq!(pending(cx, &shell), 2, "Refresh answers nothing");
    click(cx, "approval-allow");
    assert_eq!(decision(cx, first), Some(ApprovalDecision::AllowOnce));
    click(cx, "approval-allow");
    assert_eq!(decision(cx, second), Some(ApprovalDecision::AllowOnce));
    click(cx, "doc-tab-approvals-close");
    assert!(bounds(cx, "approvals-view").is_none());
    present(cx, "document");
}

#[gpui::test]
fn the_detail_shows_what_it_will_send_from_the_open_workspace_masked(cx: &mut TestAppContext) {
    let (shell, cx, automation, _dirs) = window(cx);
    let client = arrive(cx, &shell, call(1, "request_execute"), 1);
    click(cx, "rail-approvals");
    for (id, text) in [
        ("approval-title", "request_execute"),
        ("approval-status", "Pending"),
        ("approval-call-id", "call 1"),
        ("approval-client", "Claude Code"),
        ("approval-environment", "prod"),
        ("approval-class", "Execute"),
        ("approval-tool", "request_execute"),
        ("approval-policy", "execute → approval"),
        (
            "approval-request",
            "Billing API › invoices › Create invoice",
        ),
        ("approval-method", "POST"),
        ("approval-url-template", "{{base_url}}/v1/invoices"),
        ("approval-url", "https://api.test/v1/invoices"),
        ("approval-resolved-in", "resolved in prod"),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(text), "{id}");
    }
    let headers = labels(&automation, "approval-header-");
    for header in [
        "X-Api-Key: ••••••••",
        "X-Tenant-Id: tn_prod_4821",
        "Authorization: Bearer ••••••••",
    ] {
        assert!(headers.iter().any(|shown| shown == header), "{headers:?}");
    }
    assert!(
        label(&automation, "approval-body").is_some_and(|body| body.contains("\"currency\"")),
        "the body preview"
    );
    let variables = labels(&automation, "approval-variable-");
    assert_eq!(
        variables,
        [
            "{{base_url}} https://api.test Billing API",
            "{{access_token}} •••••••• SECRET keyring",
            "{{tenant_id}} tn_prod_4821 prod",
        ]
    );
    assert_eq!(
        label(&automation, "approval-reason").as_deref(),
        Some("Reason (sent back to the agent when denying)")
    );
    for (id, name) in [
        ("approval-deny", "Deny"),
        ("approval-allow-session", "Allow for this session"),
        ("approval-allow", "Allow once"),
    ] {
        assert_eq!(label(&automation, id).as_deref(), Some(name), "{id}");
        present(cx, &format!("{id}-key"));
    }
    assert!(
        label(&automation, "approval-footnote")
            .is_some_and(|text| text.starts_with("Allow once sends this call only")),
        "the footnote"
    );
    let tree = format!("{:?}", automation.snapshot());
    assert!(!tree.contains(SECRET), "a secret reached the tree");
    assert!(!tree.contains("SUPERSECRET"), "a secret reached the tree");

    click(cx, "approval-deny");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::Deny));
}

#[gpui::test]
fn a_call_on_another_workspace_shows_only_what_it_carries(cx: &mut TestAppContext) {
    let (shell, cx, automation, _dirs) = window(cx);
    let mut other = call(1, "request_execute");
    other.workspace = Some(WorkspaceRef {
        id: "01J00000000000000000000999".to_owned(),
        name: "Elsewhere".to_owned(),
    });
    other.url = Some("https://other.test/x?key=••••••••".to_owned());
    let client = arrive(cx, &shell, other, 1);
    click(cx, "rail-approvals");
    assert!(
        label(&automation, "approval-unavailable")
            .is_some_and(|text| text.contains("Elsewhere") && text.contains("not available")),
        "{:?}",
        label(&automation, "approval-unavailable")
    );
    assert_eq!(
        label(&automation, "approval-method").as_deref(),
        Some("POST")
    );
    assert_eq!(
        label(&automation, "approval-url").as_deref(),
        Some("https://other.test/x?key=••••••••")
    );
    assert_eq!(
        label(&automation, "approval-targets").as_deref(),
        Some("Create invoice")
    );
    assert_eq!(
        label(&automation, "approval-request").as_deref(),
        Some("Create invoice")
    );
    assert!(bounds(cx, "approval-url-template").is_none());
    assert!(bounds(cx, "approval-variables").is_none());
    assert_eq!(
        label(&automation, "approvals-row-1-request").as_deref(),
        Some("POST /x · prod")
    );
    click(cx, "approval-allow");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::AllowOnce));
}

#[gpui::test]
fn a_write_shows_its_target_and_no_send(cx: &mut TestAppContext) {
    let (shell, cx, automation, _dirs) = window(cx);
    let client = arrive(cx, &shell, call(1, "request_update"), 1);
    click(cx, "rail-approvals");
    assert_eq!(
        label(&automation, "approval-request").as_deref(),
        Some("Billing API › invoices › Create invoice")
    );
    assert_eq!(
        label(&automation, "approval-client").as_deref(),
        Some("an agent")
    );
    assert_eq!(
        label(&automation, "approval-environment").as_deref(),
        Some("None")
    );
    assert!(bounds(cx, "approval-sends").is_none());
    assert!(bounds(cx, "approval-unavailable").is_none());
    click(cx, "approval-allow-session");
    assert_eq!(decision(cx, client), Some(ApprovalDecision::AllowSession));
}

#[gpui::test]
fn d_denies_s_allows_for_the_session_a_allows_once_but_not_in_the_reason(cx: &mut TestAppContext) {
    let (shell, cx, _automation, _dirs) = window(cx);
    let first = arrive(cx, &shell, call(1, "request_execute"), 1);
    let second = arrive(cx, &shell, call(2, "request_update"), 2);
    let third = arrive(cx, &shell, call(3, "request_update"), 3);
    click(cx, "rail-approvals");

    // Typed in the reason field, the keys are text.
    click(cx, "approval-reason");
    cx.simulate_input("das");
    for _ in 0..3 {
        cx.executor().advance_clock(POLL);
        cx.run_until_parked();
    }
    assert_eq!(pending(cx, &shell), 3);

    click(cx, "approvals-row-1");
    cx.simulate_keystrokes("d");
    let denied = answer(cx, first).ok();
    assert_eq!(
        denied.as_ref().map(|response| response.decision),
        Some(ApprovalDecision::Deny)
    );
    assert_eq!(
        denied.and_then(|response| response.reason).as_deref(),
        Some("das"),
        "the reason typed before reaches the agent"
    );
    cx.simulate_keystrokes("s");
    assert_eq!(decision(cx, second), Some(ApprovalDecision::AllowSession));
    cx.simulate_keystrokes("a");
    assert_eq!(decision(cx, third), Some(ApprovalDecision::AllowOnce));
    present(cx, "approvals-empty");
}
