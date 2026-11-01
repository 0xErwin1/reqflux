//! The activity rail on the desk: one button per view, Settings at the bottom. Collections
//! and History swap what the sidebar island shows; Environments opens the workspace settings
//! page at its Environments section; MCP approvals opens the approvals view and carries the
//! pending count as a badge, `rail-approvals-count`; Audit log opens the audit view. The
//! active one is marked; Settings opens the settings page at App > General and is the one
//! marked while the page shows an App section (Environments is, for a workspace section).
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
        "audit" => Some(Panel::Audit),
        _ => None,
    }
}

/// Whether the rail entry `id` is marked while `panel` shows; `app_settings` when the settings
/// page shows an App section, which marks Settings instead of Environments.
fn is_active(id: &str, panel: Panel, app_settings: bool) -> bool {
    if id == entry_id(SETTINGS) {
        return app_settings;
    }
    !(app_settings && panel == Panel::Environments) && target(id) == Some(panel)
}

/// The rail with the button of `panel` active and `pending` approvals on its badge;
/// `app_settings` when the settings page shows an App section.
pub(super) fn render(
    panel: Panel,
    app_settings: bool,
    pending: usize,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    let mut entries: Vec<RailEntry> = VIEWS
        .iter()
        .map(|&(id, icon, label)| {
            let id = entry_id(id);
            let target = target(id);
            let entry = RailEntry::new(id, ui::icon(icon), label).active(is_active(
                id,
                panel,
                app_settings,
            ));
            if target == Some(Panel::Approvals) {
                entry.count(pending, BellUrgency::Approval)
            } else {
                entry
            }
        })
        .collect();
    entries.push(
        RailEntry::new(entry_id(SETTINGS), ui::icon(IconName::Settings), "Settings")
            .active(is_active(entry_id(SETTINGS), panel, app_settings))
            .bottom(),
    );
    let shell = cx.entity().downgrade();
    ActivityRail::new("activity-rail", entries).on_select(move |id: &SharedString, window, cx| {
        if id.as_ref() == entry_id(SETTINGS) {
            if let Err(error) = shell.update(cx, |this, cx| this.open_app_settings(window, cx)) {
                log::warn!("the rail outlived the shell: {error}");
            }
            return;
        }
        let Some(target) = target(id) else {
            return;
        };
        let updated = shell.update(cx, |this, cx| match target {
            Panel::Approvals => this.open_approvals(window, cx),
            Panel::Audit => this.open_audit(window, cx),
            Panel::Environments => this.open_settings(Section::Environments(None), cx),
            target => this.show_panel(target, cx),
        });
        if let Err(error) = updated {
            log::warn!("the rail outlived the shell: {error}");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(panel: Panel, app_settings: bool) -> Vec<&'static str> {
        VIEWS
            .iter()
            .map(|&(id, _, _)| id)
            .chain([SETTINGS])
            .filter(|id| is_active(entry_id(id), panel, app_settings))
            .collect()
    }

    #[test]
    fn the_app_section_marks_settings_and_a_workspace_section_environments() {
        assert_eq!(marked(Panel::Environments, true), ["rail-settings"]);
        assert_eq!(marked(Panel::Environments, false), ["rail-environments"]);
    }

    #[test]
    fn every_other_view_marks_its_own_button() {
        assert_eq!(marked(Panel::Collections, false), ["rail-collections"]);
        assert_eq!(marked(Panel::History, false), ["rail-history"]);
        assert_eq!(marked(Panel::Approvals, false), ["rail-approvals"]);
        assert_eq!(marked(Panel::Audit, false), ["rail-audit"]);
    }
}
