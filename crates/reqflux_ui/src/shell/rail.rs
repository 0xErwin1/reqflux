//! The activity rail on the desk: one button per view, Settings at the bottom. Collections,
//! History and Environments swap what the sidebar island shows; MCP approvals opens the
//! approvals view and carries the pending count as a badge. The active one is marked; the
//! others do nothing yet.

use bolt::IconName;
use bolt::controls::RailButton;
use bolt::tokens::{chrome, space};
use gpui::{Context, IntoElement, ParentElement, Styled, div};

use super::sidebar_history::Panel;
use super::{Shell, region};

/// The views of the rail above the spacer: element id, icon and label.
pub(crate) const VIEWS: [(&str, IconName, &str); 5] = [
    ("rail-collections", IconName::Box, "Collections"),
    ("rail-history", IconName::RotateCcwClock, "History"),
    ("rail-environments", IconName::Layers, "Environments"),
    ("rail-approvals", IconName::Bot, "MCP approvals"),
    ("rail-audit", IconName::FingerprintPattern, "Audit log"),
];

/// The element id of the Settings button at the bottom of the rail.
pub(crate) const SETTINGS: &str = "rail-settings";

/// The rail with the button of `panel` active and `pending` approvals on its badge.
pub(super) fn render(panel: Panel, pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let buttons: Vec<_> = VIEWS
        .iter()
        .map(|&(id, icon, label)| {
            let target = match id {
                "rail-collections" => Some(Panel::Collections),
                "rail-history" => Some(Panel::History),
                "rail-environments" => Some(Panel::Variables),
                "rail-approvals" => Some(Panel::Approvals),
                _ => None,
            };
            let button = RailButton::new(id, icon, label).active(target == Some(panel));
            match target {
                Some(Panel::Approvals) => button.badge(Some(pending)).on_click(
                    cx.listener(|this, _event, window, cx| this.open_approvals(window, cx)),
                ),
                Some(target) => button.on_click(cx.listener(move |this, _event, _, cx| {
                    this.show_panel(target, cx);
                })),
                None => button,
            }
        })
        .collect();
    region("activity-rail")
        .flex_none()
        .w(chrome::RAIL_WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(space::HALF)
        .pt(space::XS)
        .pb(space::XS)
        .children(buttons)
        .child(div().flex_1())
        .child(RailButton::new(SETTINGS, IconName::Settings, "Settings"))
}
