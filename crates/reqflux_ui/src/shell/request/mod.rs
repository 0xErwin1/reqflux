//! The open request under the document tabs: the request line, the sub-tab bar and the pane
//! of the selected sub-tab: the Params and Headers grids, the Body, Auth and Settings panes, or
//! the Scripts pane.

mod auth;
mod auth_menu;
mod body;
mod body_menu;
mod code;
mod grid;
mod grid_model;
mod line;
mod menu;
mod method_menu;
mod model;
pub(super) mod option_menu;
mod pane_model;
mod scripts;
pub(super) mod segmented;
mod send;
mod settings;
pub(super) mod spans;
pub(super) mod sub_tabs;
pub(super) mod ui;

pub(crate) use grid::secret;
#[cfg(test)]
pub(crate) use model::count;
pub(crate) use model::{RequestState, SubTab};
pub(crate) use send::{Completed, Failure, Received, SendState, StatusClass};
pub(crate) use send::{elapsed_text, size_text};
#[cfg(test)]
pub(crate) use sub_tabs::{shown_rows, sub_tab_badge};

use bolt::tokens::Spacing;
use gpui::{Context, Div, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window};

use super::{Shell, region};

/// The request `id` with its `state`, at its natural height over the response; `sending`
/// while its send is in flight.
pub(super) fn render(
    id: &str,
    state: &RequestState,
    sending: bool,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> [Stateful<Div>; 3] {
    [
        line::render(id, state, sending, shell, window, cx),
        sub_tabs::render(id, state, shell, cx),
        pane(id, state, shell, window, cx),
    ]
}

fn pane(
    id: &str,
    state: &RequestState,
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let inspection = &state.inspection;
    // The panes show the draft's request when the request has one, so an edit is visible
    // immediately and survives a restart.
    let request = shell.displayed_request(id, &inspection.request.request);
    let content = match state.tab {
        SubTab::Params => vec![grid::render(
            inspection,
            "params",
            "param",
            &request.params,
            &[],
            Some(id),
            shell,
            cx,
        )],
        SubTab::Headers => vec![grid::render(
            inspection,
            "headers",
            "header",
            &request.headers,
            &[],
            Some(id),
            shell,
            cx,
        )],
        SubTab::Body => body::render(id, inspection, &request.body, shell, window, cx),
        SubTab::Auth => auth::render(id, inspection, &request.auth, shell, window, cx),
        SubTab::Scripts => scripts::render(id, inspection, shell, cx),
        SubTab::Settings => settings::render(id, &request.settings, shell, cx),
    };
    // The pane takes what the split leaves and scrolls: a tall tab (Scripts, a long grid)
    // must not run under the response.
    region("request-pane")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pb(Spacing::MD)
        .children(content)
}
