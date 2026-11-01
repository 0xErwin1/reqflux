//! ReqFlux's answers to `bolt`'s text seam (shared-bolt S1).

use bolt::controls::Dropdown;
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, TestAppContext,
    Window, div,
};

use super::{text, text_with};

/// Every key `bolt` reads through `bolt::strings` at the pinned rev (`strings::text`,
/// `strings::text_with` and the `*_KEY` constants of its controls, toasts, command palette
/// and chart legend).
const BOLT_KEYS: [&str; 25] = [
    "controls.dropdown.placeholder",
    "controls.multi_select.placeholder",
    "controls.multi_select.clear_all",
    "controls.multi_select.summary_all",
    "controls.multi_select.summary_count",
    "controls.multi_select.more.one",
    "controls.multi_select.more.many",
    "toast.action.dismiss",
    "toast.action.show_details",
    "toast.action.hide_details",
    "toast.stack.hidden_more",
    "palette.search.placeholder",
    "palette.empty",
    "palette.footer.navigate",
    "palette.footer.run",
    "palette.footer.open_in_new_tab",
    "palette.footer.commands_only",
    "palette.footer.tables_only",
    "palette.section.connections",
    "palette.section.commands",
    "palette.section.charts",
    "palette.section.tables",
    "palette.section.scripts",
    "chart.legend.hide_series",
    "chart.legend.show_series",
];

#[test]
fn every_key_bolt_uses_is_answered() {
    let unanswered: Vec<&str> = BOLT_KEYS
        .into_iter()
        .filter(|key| text(key).as_ref() == *key || text(key).is_empty())
        .collect();
    assert!(unanswered.is_empty(), "unanswered: {unanswered:?}");
}

#[test]
fn an_unknown_key_answers_itself() {
    assert_eq!(
        text("bolt.surface.not_yet_known").as_ref(),
        "bolt.surface.not_yet_known"
    );
    assert_eq!(
        text_with("bolt.surface.not_yet_known", &[("n", "1")]),
        "bolt.surface.not_yet_known"
    );
}

#[test]
fn arguments_are_interpolated() {
    assert_eq!(text("controls.dropdown.placeholder").as_ref(), "Select");
    assert_eq!(
        text_with(
            "controls.multi_select.summary_count",
            &[("name", "Class"), ("count", "2")]
        ),
        "Class · 2"
    );
    assert_eq!(
        text_with("controls.multi_select.summary_all", &[("name", "Outcome")]),
        "Outcome · all"
    );
    assert_eq!(
        text_with("toast.stack.hidden_more", &[("count", "3")]),
        "3 more"
    );
}

struct Host(Entity<Dropdown>);

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.0.clone())
    }
}

#[gpui::test]
fn a_dropdown_without_a_selection_shows_its_placeholder(cx: &mut TestAppContext) {
    cx.update(|cx| crate::theme::install(crate::theme::PaletteMode::Dark, cx));
    let (_host, cx) = cx.add_window_view(|_, cx| {
        let dropdown = cx.new(|_| Dropdown::new("strings-dropdown"));
        Host(dropdown)
    });
    let automation = gpui_mcp::Automation::isolated();
    cx.update(|window, _| automation.attach(window));
    cx.run_until_parked();
    let labels: Vec<String> = automation
        .snapshot()
        .nodes
        .values()
        .filter_map(|node| node.label.clone())
        .collect();
    assert!(
        labels.iter().any(|label| label == "Select"),
        "labels: {labels:?}"
    );
}
