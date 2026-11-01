//! The activity rail on the desk: one button per view, Settings at the bottom. Collections
//! and History swap what the sidebar island shows; Environments opens the workspace settings
//! page at its Environments section; MCP approvals opens the approvals view and carries the
//! pending count as a badge, `rail-approvals-count`. The active one is marked; the others do
//! nothing yet.
//!
//! The rail is `bolt`'s activity rail, as DBFlux draws it.

use bolt::composites::{ActivityRail, BellUrgency, RailEntry};
use gpui::{Context, IntoElement, SharedString};
use gpui_kit_assets::IconName;

use super::Shell;
use super::request::ui;
use super::settings_page::Section;
use super::sidebar_history::Panel;

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

/// The rail entry id of the element id `id`: the rail prefixes its entries with `rail-`.
fn entry_id(id: &'static str) -> &'static str {
    id.strip_prefix("rail-").unwrap_or(id)
}

/// The panel the rail button `id` shows, if it shows one.
fn target(id: &str) -> Option<Panel> {
    match id {
        "collections" => Some(Panel::Collections),
        "history" => Some(Panel::History),
        "environments" => Some(Panel::Environments),
        "approvals" => Some(Panel::Approvals),
        _ => None,
    }
}

/// The rail with the button of `panel` active and `pending` approvals on its badge.
pub(super) fn render(panel: Panel, pending: usize, cx: &mut Context<Shell>) -> impl IntoElement {
    let mut entries: Vec<RailEntry> = VIEWS
        .iter()
        .map(|&(id, icon, label)| {
            let id = entry_id(id);
            let target = target(id);
            let entry = RailEntry::new(id, ui::icon(icon), label).active(target == Some(panel));
            if target == Some(Panel::Approvals) {
                entry.count(pending, BellUrgency::Approval)
            } else {
                entry
            }
        })
        .collect();
    entries.push(
        RailEntry::new(entry_id(SETTINGS), ui::icon(IconName::Settings), "Settings").bottom(),
    );
    let shell = cx.entity().downgrade();
    ActivityRail::new("activity-rail", entries).on_select(move |id: &SharedString, window, cx| {
        let Some(target) = target(id) else {
            return;
        };
        let updated = shell.update(cx, |this, cx| match target {
            Panel::Approvals => this.open_approvals(window, cx),
            Panel::Environments => this.open_settings(Section::Environments(None), cx),
            target => this.show_panel(target, cx),
        });
        if let Err(error) = updated {
            log::warn!("the rail outlived the shell: {error}");
        }
    })
}
