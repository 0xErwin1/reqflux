//! The Settings pane, as artboard request-panes/01 frame C draws it: one row per setting
//! with its label, its control, its badge (Default or Overridden), a Reset link when it is
//! overridden, and a hint. The timeout is a number well that opens a field on click, empty
//! for the default; redirects and TLS verification are Default / On / Off radio groups, the
//! engine's tri-state. Every edit lands in the request's draft.

mod controls;

use bolt::controls::{GpuiInput, InputEnter, InputEscape, InputState};
use bolt::primitives::{Chamfer, ChamferRing};
use bolt::tokens::{ChamferCut, Fields, Spacing};
use gpui::{AnyElement, Context, Entity, ParentElement, Styled, div, prelude::*};
use reqflux_core::model::Settings;
use reqflux_engine::DEFAULT_TIMEOUT_MS;

use self::controls::{Badge, badge, choice, hint, link, radio_group, row};
use super::body::controls::well;
use super::pane_model::settings_rows;
use crate::shell::Shell;
use crate::shell::rows::part;
use crate::shell::setting_edit::Toggle;
use crate::tokens;

/// The Default / On / Off choices, by id part, name and value.
const CHOICES: [(&str, &str, Option<bool>); 3] = [
    ("default", "Default", None),
    ("on", "On", Some(true)),
    ("off", "Off", Some(false)),
];

pub(super) fn render(
    request_id: &str,
    settings: &Settings,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    let editing = shell
        .cell_edit
        .as_ref()
        .filter(|_| shell.timeout_edit_of(request_id))
        .map(|edit| edit.input.clone());
    let rows = settings_rows(settings).map(|(id, label, shown)| match id {
        "request-setting-timeout" => {
            let control = timeout_control(request_id, settings, editing.clone(), cx);
            let muted = bolt::colors(cx).muted;
            let text = format!(
                "Default {DEFAULT_TIMEOUT_MS} ms. Bounds the whole redirect chain. Empty uses \
                 the default."
            );
            row(
                "request-setting-timeout-row".to_owned(),
                label,
                &shown,
                control,
                hint("request-setting-timeout-hint".to_owned(), text, muted, cx),
                cx,
            )
            .into_any_element()
        }
        "request-setting-redirects" => toggle_row(
            request_id,
            id,
            label,
            &shown,
            Toggle::FollowRedirects,
            settings,
            cx,
        ),
        "request-setting-tls" => toggle_row(
            request_id,
            id,
            label,
            &shown,
            Toggle::VerifyTls,
            settings,
            cx,
        ),
        // A row this pane does not know how to edit stays read-only rather than silently
        // editing another setting.
        _ => {
            let muted = bolt::colors(cx).muted;
            row(
                id.to_owned(),
                label,
                &shown,
                div().child(shown.clone()),
                hint(format!("{id}-hint"), String::new(), muted, cx),
                cx,
            )
            .into_any_element()
        }
    });
    vec![
        div()
            .pt(Spacing::SM)
            .pl(Spacing::XL)
            .pr(Spacing::XL)
            .children(rows)
            .into_any_element(),
    ]
}

/// The timeout well `request-setting-timeout`, or the open field
/// `request-setting-timeout-input`, then its badge and, when overridden, its Reset link
/// `request-setting-timeout-reset`.
fn timeout_control(
    request_id: &str,
    settings: &Settings,
    editing: Option<Entity<InputState>>,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let tint = bolt::brand::brand(cx).accent_tint;
    let unit = div().text_color(colors.muted).child("ms");
    let field = match editing {
        Some(input) => well("request-setting-timeout-input".to_owned(), cx)
            .w(tokens::SETTING_NUMBER_FIELD)
            .child(Chamfer::new(ChamferCut::CONTROL).ring(ChamferRing::outline(tint)))
            .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
                // Ctrl+Enter maps to the input's `secondary-enter` while the field has
                // focus: it sends instead of committing.
                if action.secondary {
                    this.send_active(window, cx);
                } else {
                    this.commit_cell_edit(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.discard_cell_edit(window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        GpuiInput::new(&input)
                            .appearance(false)
                            .bordered(false)
                            .p_0()
                            .h_full()
                            .text_size(Fields::TEXT)
                            .font_family(bolt::fonts::editor_family(cx))
                            .text_color(colors.foreground),
                    ),
            )
            .child(unit),
        None => {
            let request = request_id.to_owned();
            let text = part("request-setting-timeout-text".to_owned())
                .flex_1()
                .h_full()
                .flex()
                .items_center();
            let value = match settings.timeout_ms {
                Some(ms) => text.child(ms.to_string()),
                None => text
                    .text_color(colors.muted)
                    .child(DEFAULT_TIMEOUT_MS.to_string()),
            };
            well("request-setting-timeout".to_owned(), cx)
                .w(tokens::SETTING_NUMBER_FIELD)
                .cursor_text()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.start_timeout_edit(&request, window, cx);
                }))
                .child(value)
                .child(unit)
        }
    };
    let overridden = settings.timeout_ms.is_some();
    let state = if overridden {
        Badge::Overridden
    } else {
        Badge::Default
    };
    let reset = overridden.then(|| {
        let request = request_id.to_owned();
        link(
            "request-setting-timeout-reset".to_owned(),
            "Reset to default",
            cx.listener(move |this, _event, window, cx| {
                this.reset_timeout(&request, window, cx);
            }),
        )
    });
    div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(field)
        .child(badge("request-setting-timeout-badge".to_owned(), state))
        .children(reset)
        .into_any_element()
}

/// The row `id` of `toggle`: the radio group `request-setting-<name>` with its choices
/// `request-setting-<name>-<default|on|off>`, the badge `request-setting-<name>-badge`, the
/// Reset link `request-setting-<name>-reset` when overridden, and the hint.
fn toggle_row(
    request_id: &str,
    id: &str,
    label: &'static str,
    shown: &str,
    toggle: Toggle,
    settings: &Settings,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let colors = bolt::colors(cx);
    let value = toggle.value(settings);
    let name = match toggle {
        Toggle::FollowRedirects => "follow-redirects",
        Toggle::VerifyTls => "verify-tls",
    };
    let choices: Vec<_> = CHOICES
        .into_iter()
        .map(|(part, choice_label, choice_value)| {
            let request = request_id.to_owned();
            // Both settings default to on.
            let note = choice_value.is_none().then_some("on");
            choice(
                format!("request-setting-{name}-{part}"),
                choice_label,
                note,
                value == choice_value,
                cx,
            )
            .on_click(cx.listener(move |this, _event, window, cx| {
                this.set_setting(&request, toggle, choice_value, window, cx);
            }))
        })
        .collect();
    let weakened = toggle == Toggle::VerifyTls && value == Some(false);
    let state = match value {
        None => Badge::Default,
        Some(_) if weakened => Badge::Warning,
        Some(_) => Badge::Overridden,
    };
    let reset = value.is_some().then(|| {
        let request = request_id.to_owned();
        link(
            format!("request-setting-{name}-reset"),
            "Reset to default",
            cx.listener(move |this, _event, window, cx| {
                this.set_setting(&request, toggle, None, window, cx);
            }),
        )
    });
    let (text, color) = match toggle {
        Toggle::FollowRedirects => (
            "Follows up to 10 redirects. Credentials are dropped when the origin changes.",
            colors.muted,
        ),
        Toggle::VerifyTls if weakened => (
            "Certificates are not checked for this request.",
            colors.warning,
        ),
        Toggle::VerifyTls => ("Checks the server certificate.", colors.muted),
    };
    let control = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(radio_group(
            format!("request-setting-{name}"),
            label,
            choices,
            cx,
        ))
        .child(badge(format!("request-setting-{name}-badge"), state))
        .children(reset);
    row(
        id.to_owned(),
        label,
        shown,
        control,
        hint(format!("{id}-hint"), text.to_owned(), color, cx),
        cx,
    )
    .into_any_element()
}
