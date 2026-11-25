//! System notifications of approvals: a request that arrives while the window is in the
//! background posts one OS notification, and clicking it opens the MCP approvals view on that
//! request. A request that arrives while the window has the focus posts none.

use gpui::{
    AppContext as _, Entity, SystemNotificationResponse, TestAppContext, VisualTestContext,
};
use gpui_component::Root;
use reqflux_engine::approvals::{ApprovalRequest, PROTOCOL_VERSION, WorkspaceRef};
use tempfile::TempDir;

use super::Shell;
use super::approvals_notice::{NOTICE_TITLE, notice_body};
use super::approvals_tests::{arrive, listen};
use super::draft_tests::state_file;
use crate::workspace::OpenedWorkspace;
use crate::workspace_tests::demo_workspace;

fn request(id: u64, client: Option<&str>) -> ApprovalRequest {
    ApprovalRequest {
        version: PROTOCOL_VERSION,
        id,
        tool: "request_execute".to_owned(),
        class: "execute".to_owned(),
        workspace: Some(WorkspaceRef {
            id: "01W".to_owned(),
            name: "Demo".to_owned(),
        }),
        targets: vec!["Ping".to_owned()],
        client: client.map(str::to_owned),
        ..ApprovalRequest::default()
    }
}

/// A shell inside gpui-component's `Root`, as the app opens it, listening for approvals.
fn window(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Vec<TempDir>) {
    cx.executor().allow_parking();
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    // As the app does at startup; the platform posts nothing without an identity.
    cx.update(|cx| cx.set_app_identity(crate::APP_ID, "ReqFlux"));
    let dir = demo_workspace();
    let (state_dir, state_path) = state_file();
    let state = match reqflux_engine::state::AppState::open(&state_path) {
        Ok(state) => state,
        Err(error) => panic!("open state: {error}"),
    };
    cx.update(super::bind_keys);
    let workspace = OpenedWorkspace::open(Some(dir.path().to_path_buf()));
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let shell = cx.new(|cx| Shell::with_state(workspace, Some(state), cx));
        made = Some(shell.clone());
        Root::new(shell, window, cx)
    });
    let Some(shell) = made else {
        panic!("the shell was not made");
    };
    cx.run_until_parked();
    let runtime = listen(cx, &shell);
    (shell, cx, vec![dir, state_dir, runtime])
}

#[test]
fn the_body_names_the_client_the_tool_the_target_and_the_workspace() {
    assert_eq!(
        notice_body(&request(1, Some("Claude Code"))),
        "Claude Code wants to run request_execute on Ping in Demo"
    );
    let mut bare = request(2, None);
    bare.targets.clear();
    bare.workspace = None;
    assert_eq!(notice_body(&bare), "An agent wants to run request_execute");
}

#[gpui::test]
fn a_request_in_the_background_posts_one_notification_that_opens_it(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    // The test platform blurs only the window it holds as the active one.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.deactivate_window();
    assert!(!cx.update(|window, _| window.is_window_active()));

    let _client = arrive(cx, &shell, request(1, Some("Claude Code")), 1);
    let shown = cx.shown_system_notifications();
    assert_eq!(shown.len(), 1, "one notification per request");
    let Some(notice) = shown.first() else {
        panic!("no notification");
    };
    assert_eq!(notice.title.as_ref(), NOTICE_TITLE);
    assert_eq!(
        notice.body.as_ref(),
        "Claude Code wants to run request_execute on Ping in Demo"
    );
    assert!(!cx.update(|_, cx| shell.read(cx).approvals_open(cx)));

    cx.simulate_system_notification_response(SystemNotificationResponse {
        tag: notice.tag.clone(),
        action_id: None,
    });
    cx.run_until_parked();
    assert!(
        cx.update(|_, cx| shell.read(cx).approvals_open(cx)),
        "clicking the notification opens the MCP approvals view"
    );
}

#[gpui::test]
fn a_request_while_the_window_has_the_focus_posts_none(cx: &mut TestAppContext) {
    let (shell, cx, _dirs) = window(cx);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();

    let _client = arrive(cx, &shell, request(1, None), 1);
    assert!(cx.shown_system_notifications().is_empty());
}
