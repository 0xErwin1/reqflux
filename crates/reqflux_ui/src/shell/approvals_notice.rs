//! System notifications of approvals: a request that arrives while the window is in the
//! background posts one OS notification ("MCP approval needed", who asks for what) and asks
//! the window manager for attention, so the user sees that an agent is waiting. Clicking the
//! notification brings the window forward and opens the MCP approvals view on that request.
//! While the window has the focus the bell and rail counts are enough and nothing is posted.
//!
//! Delivery goes through gpui-component's notification list (system-only), which owns the
//! app's single system-notification response handler and activates the window on a click.

use gpui::{Context, Window};
use gpui_component::{Root, WindowExt as _, notification::Notification};
use reqflux_engine::approvals::ApprovalRequest;

use super::Shell;

/// The title of the system notification.
pub(crate) const NOTICE_TITLE: &str = "MCP approval needed";

/// Marks the notifications of approvals; each one is keyed by the request's window key.
struct ApprovalNotice;

/// "<client> wants to run <tool>[ on <targets>][ in <workspace>]", the client "An agent" when
/// it gave no name. Only names cross the socket, so nothing here is secret.
pub(crate) fn notice_body(request: &ApprovalRequest) -> String {
    let client = request
        .client
        .as_deref()
        .filter(|client| !client.trim().is_empty())
        .unwrap_or("An agent");
    let mut body = format!("{client} wants to run {}", request.tool);
    if !request.targets.is_empty() {
        body.push_str(" on ");
        body.push_str(&request.targets.join(", "));
    }
    if let Some(workspace) = &request.workspace {
        body.push_str(" in ");
        body.push_str(&workspace.name);
    }
    body
}

impl Shell {
    /// Posts the system notification of the request queued under `key` when the window is
    /// in the background, and asks for the window manager's attention.
    pub(super) fn notify_approval(
        &mut self,
        key: u64,
        request: &ApprovalRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.is_window_active() {
            return;
        }
        window.request_attention();
        // The notification list lives in gpui-component's Root, which the app always wraps
        // the shell in; a bare shell (some tests) has no list to post through.
        if !matches!(window.root::<Root>(), Some(Some(_))) {
            log::debug!("no notification list for the approval of {}", request.tool);
            return;
        }
        let shell = cx.entity().downgrade();
        let notice = Notification::new()
            .id1::<ApprovalNotice>(key)
            .title(NOTICE_TITLE)
            .message(notice_body(request))
            .system()
            .on_click(move |_, window, cx| {
                if let Err(error) = shell.update(cx, |shell, cx| {
                    shell.open_approvals(window, cx);
                    shell.select_approval_key(key, window, cx);
                }) {
                    log::debug!("the window of an approval notification is gone: {error}");
                }
            });
        window.push_notification(notice, cx);
    }
}
