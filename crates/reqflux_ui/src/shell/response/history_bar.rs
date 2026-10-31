//! The bar over a response reopened from history. It draws nothing: the history tab's
//! banner (`shell::history`) carries the recorded moment, Open source request and Resend, as
//! design/history/01-sidebar-readonly-tab draws them, so the response under it keeps only its
//! own tab row.

use gpui::{Context, Div, div};

use crate::shell::Shell;
use crate::shell::history::Reopened;

/// The bar of `reopened` in the response of `id`: empty.
pub(super) fn render(_id: &str, _reopened: &Reopened, _cx: &mut Context<Shell>) -> Div {
    div()
}
