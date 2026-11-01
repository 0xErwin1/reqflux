//! The activity rail on the desk: one button per view, Settings at the bottom. Collections
//! and History swap what the sidebar island shows; Environments opens the workspace settings
//! page at its Environments section; MCP approvals opens the approvals view and carries the
//! pending count as a badge. The active one is marked; the others do nothing yet.
//!
//! The rail is drawn as DBFlux's activity rail draws it.
// bolt gap: `bolt::composites::ActivityRail` names its count badge `rail-<id>-count` with
// no accessible label (the count leaves the automation tree), sets no selector on its root
// and no Button role on its buttons; this module goes once it does.

use bolt::primitives::{Chamfer, Icon};
use bolt::tokens::{ButtonMetrics, ChamferCut, ShellMetrics};
use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::*,
};
use gpui_component::tooltip::Tooltip;
use gpui_kit_assets::IconName;

use super::request::ui;
use super::settings_page::Section;
use super::sidebar_history::Panel;
use super::{Shell, region, text_region};

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

/// What a rail button runs when clicked.
type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The most a count badge shows; a larger count reads `99+`.
const BADGE_MAX: usize = 99;

/// The text of a count badge for `count`, `None` at zero.
pub(super) fn badge_text(count: usize) -> Option<String> {
    match count {
        0 => None,
        count if count > BADGE_MAX => Some(format!("{BADGE_MAX}+")),
        count => Some(count.to_string()),
    }
}

/// The count badge `id` reading `text`: the accent keycap chip DBFlux puts on its bell, which
/// the automation tree reports with the count as its text. The caller places it.
pub(super) fn count_badge(id: &'static str, text: String, cx: &App) -> gpui::Stateful<gpui::Div> {
    let brand = bolt::brand::brand(cx);
    text_region(id)
        .aria_label(text.clone())
        .absolute()
        .flex()
        .items_center()
        .justify_center()
        .min_w(ShellMetrics::BELL_BADGE_MIN_WIDTH)
        .h(ShellMetrics::BELL_BADGE_HEIGHT)
        .pl(ShellMetrics::BELL_BADGE_PADDING_X)
        .pr(ShellMetrics::BELL_BADGE_PADDING_X)
        .font_family(bolt::fonts::ui_family(cx))
        .text_size(ShellMetrics::BELL_BADGE_FONT)
        .font_weight(FontWeight::BOLD)
        .text_color(brand.on_accent)
        .child(Chamfer::new(ChamferCut::KEYCAP).fill(brand.accent))
        .child(div().relative().child(text))
}

/// A rail button `id` with `icon`, named `label`: the tint wash and a tint icon when
/// `active`, else transparent with a muted icon and the raised fill under the pointer.
fn button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    active: bool,
    badge: Option<String>,
    on_click: Option<ClickHandler>,
    cx: &App,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let shape = Chamfer::new(ChamferCut::CONTROL);
    let (shape, icon_color) = if active {
        (
            shape
                .fill(tint.opacity(ShellMetrics::RAIL_ACTIVE_ALPHA))
                .fill_hover(tint.opacity(ButtonMetrics::SOFT_FILL_HOVER))
                .fill_active(tint.opacity(ButtonMetrics::SOFT_FILL_PRESSED)),
            tint,
        )
    } else {
        (
            shape
                .fill(gpui::transparent_black())
                .fill_hover(colors.raised)
                .fill_active(colors.raised),
            colors.muted,
        )
    };
    region(id)
        .role(Role::Button)
        .aria_label(label)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(ShellMetrics::RAIL_BUTTON)
        .cursor_pointer()
        .child(shape.interactive("rail-button-chamfer"))
        .child(
            Icon::new(ui::icon(icon))
                .size(ShellMetrics::RAIL_ICON)
                .color(icon_color),
        )
        .children(badge.map(|text| {
            count_badge("rail-approvals-badge", text, cx)
                .top_0()
                .right_0()
        }))
        .tooltip(move |window, cx| Tooltip::new(SharedString::from(label)).build(window, cx))
        .when_some(on_click, |button, handler| {
            button.on_click(move |event, window, cx| handler(event, window, cx))
        })
        .into_any_element()
}

/// The rail with the button of `panel` active and `pending` approvals on its badge.
pub(super) fn render(panel: Panel, pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let buttons: Vec<_> = VIEWS
        .iter()
        .map(|&(id, icon, label)| {
            let target = match id {
                "rail-collections" => Some(Panel::Collections),
                "rail-history" => Some(Panel::History),
                "rail-environments" => Some(Panel::Environments),
                "rail-approvals" => Some(Panel::Approvals),
                _ => None,
            };
            let active = target == Some(panel);
            let (badge, on_click): (Option<String>, Option<ClickHandler>) = match target {
                Some(Panel::Approvals) => (
                    badge_text(pending),
                    Some(Box::new(cx.listener(|this, _event, window, cx| {
                        this.open_approvals(window, cx);
                    }))),
                ),
                Some(Panel::Environments) => (
                    None,
                    Some(Box::new(cx.listener(|this, _event, _, cx| {
                        this.open_settings(Section::Environments(None), cx);
                    }))),
                ),
                Some(target) => (
                    None,
                    Some(Box::new(cx.listener(move |this, _event, _, cx| {
                        this.show_panel(target, cx);
                    }))),
                ),
                None => (None, None),
            };
            button(id, icon, label, active, badge, on_click, cx)
        })
        .collect();
    region("activity-rail")
        .flex_none()
        .w(ShellMetrics::RAIL_WIDTH)
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(ShellMetrics::RAIL_GAP)
        .pt(ShellMetrics::RAIL_PADDING_Y)
        .pb(ShellMetrics::RAIL_PADDING_Y)
        .children(buttons)
        .child(div().flex_1())
        .child(button(
            SETTINGS,
            IconName::Settings,
            "Settings",
            false,
            None,
            None,
            cx,
        ))
}
