//! The sub-tab bar: Params, Headers, Body, Auth, Scripts and Settings, with the row counts of
//! Params and Headers, the scripts the request has and its overridden settings (artboard
//! request-panes/01), and at the right the problem of the inspection or else the unresolved
//! variables notice.

use bolt_legacy::chamfer::Chamfer;
use bolt_legacy::tokens::{border, chrome, cut, family, icon, space, text};
use bolt_legacy::{Icon, IconName};
use gpui::{
    Context, Div, FontWeight, Hsla, ParentElement, Role, Stateful, Styled, div, prelude::*,
};

use super::model::{RequestState, SubTab, count, unresolved_text};
use crate::shell::rows::part;
use crate::shell::{Shell, region, text as text_role};
use crate::tokens;

/// The bar, element id `request-tabs`, for the request `id`.
pub(super) fn render(
    id: &str,
    state: &RequestState,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    let notice = match &state.inspection.problem {
        Some(problem) => Some(notice("request-problem", problem.clone(), colors.danger)),
        None => unresolved_text(state.inspection.unresolved.len())
            .map(|label| notice("request-unresolved", label, colors.warning)),
    };
    region("request-tabs")
        .flex_none()
        .h(tokens::SUB_TAB_BAR)
        .flex()
        .items_center()
        .gap(space::XS)
        .pl(space::MD)
        .pr(space::MD)
        .border_t(border::THIN)
        .border_color(colors.line)
        .children(
            SubTab::ALL
                .map(|tab| sub_tab(id, state, sub_tab_badge(id, state, shell, tab), tab, cx)),
        )
        .child(div().flex_1())
        .children(notice)
}

/// The number of enabled rows `tab` shows next to its label: counted on the draft's rows
/// when the request has one, so toggling a row moves the number, and on the file's
/// otherwise.
pub(crate) fn shown_rows(
    id: &str,
    state: &RequestState,
    shell: &Shell,
    tab: SubTab,
) -> Option<usize> {
    let request = shell.displayed_request(id, &state.inspection.request.request);
    count(request, tab)
}

/// The badge next to the label of `tab`, from what the request `id` shows (its draft when it
/// has one): the enabled rows of Params and Headers, `pre`, `test` or `2` for the scripts
/// Scripts has, and the number of overridden settings. `None` for nothing to show.
pub(crate) fn sub_tab_badge(
    id: &str,
    state: &RequestState,
    shell: &Shell,
    tab: SubTab,
) -> Option<String> {
    match tab {
        SubTab::Scripts => {
            let scripts = shell.displayed_scripts(id, &state.inspection.request.scripts);
            let has = |script: &Option<String>| script.as_deref().is_some_and(|s| !s.is_empty());
            match (has(&scripts.pre_request), has(&scripts.test)) {
                (true, true) => Some("2".to_owned()),
                (true, false) => Some("pre".to_owned()),
                (false, true) => Some("test".to_owned()),
                (false, false) => None,
            }
        }
        SubTab::Settings => {
            let settings = shell
                .displayed_request(id, &state.inspection.request.request)
                .settings;
            let overridden = [
                settings.timeout_ms.is_some(),
                settings.follow_redirects.is_some(),
                settings.verify_tls.is_some(),
            ]
            .into_iter()
            .filter(|set| *set)
            .count();
            (overridden > 0).then(|| overridden.to_string())
        }
        _ => shown_rows(id, state, shell, tab).map(|rows| rows.to_string()),
    }
}

/// A notice at the right of the bar, element id `id`, in `color`.
fn notice(id: &'static str, label: String, color: Hsla) -> Stateful<Div> {
    region(id)
        .role(Role::Label)
        .aria_label(label.clone())
        .flex_none()
        .flex()
        .items_center()
        .gap(space::HALF)
        .whitespace_nowrap()
        .text_size(tokens::NOTICE_TEXT)
        .text_color(color)
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(icon::CHIP)
                .color(color),
        )
        .child(label)
}

fn icon_of(tab: SubTab) -> IconName {
    match tab {
        SubTab::Params => IconName::Hash,
        SubTab::Headers => IconName::List,
        SubTab::Body => IconName::Braces,
        SubTab::Auth => IconName::KeyRound,
        SubTab::Scripts => IconName::Code,
        SubTab::Settings => IconName::SlidersHorizontal,
    }
}

/// The sub-tab `tab`, element id `request-tab-<name>`, selected when it is the state's tab,
/// with `badge` after its label as `request-tab-<name>-count`: in the accent for the
/// overridden settings, muted otherwise.
fn sub_tab(
    id: &str,
    state: &RequestState,
    badge: Option<String>,
    tab: SubTab,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let colors = bolt_legacy::colors(cx);
    let selected = state.tab == tab;
    let (label_color, weight, icon_color) = if selected {
        (
            colors.strong,
            FontWeight::SEMIBOLD,
            bolt_legacy::brand(cx).accent_tint,
        )
    } else {
        (colors.muted, text::BODY.weight, colors.muted)
    };
    let badge_color = match tab {
        SubTab::Settings => bolt_legacy::brand(cx).accent_tint,
        _ => colors.muted,
    };
    let request = id.to_owned();
    text_role::role(part(tab.element_id()), text::BODY, cx)
        .role(Role::Tab)
        .aria_label(tab.label())
        .aria_selected(selected)
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(space::SM)
        .h(chrome::DOC_TAB)
        .pl(space::MD)
        .pr(space::MD)
        .whitespace_nowrap()
        .cursor_pointer()
        .when(selected, |chip| {
            chip.child(Chamfer::new(cut::CONTROL).fill(colors.raised))
        })
        .child(Icon::new(icon_of(tab)).size(icon::MD).color(icon_color))
        .child(
            div()
                .text_color(label_color)
                .font_weight(weight)
                .child(tab.label()),
        )
        .children(badge.map(|badge| {
            text_role::role(
                part(format!("request-tab-{}-count", tab.label().to_lowercase())),
                text::LABEL,
                cx,
            )
            .role(Role::Label)
            .aria_label(badge.clone())
            .font_family(family::MONO)
            .text_size(tokens::SMALL_MONO_TEXT)
            .text_color(badge_color)
            .child(badge)
        }))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.select_sub_tab(&request, tab, cx);
        }))
}
