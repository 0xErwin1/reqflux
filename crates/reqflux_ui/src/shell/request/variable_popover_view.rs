//! The variable popover as design/variable-popover/01-inline-popover.html draws it, on
//! `bolt`'s menu frame: the name with the winning scope badge, the value field, the initial
//! value and the scope it overrides, and a footer with the Enter and Esc keycaps and
//! Open in Environments. A secret shows its SECRET badge and a reveal eye; an undefined name
//! the quick add (the "Add to" dropdown, value, Secret box); with no active environment, the
//! environments that declare it.

use bolt::composites::{MenuItem, menu_frame, menu_row};
use bolt::controls::{
    Button, ButtonSize, Checkbox, Input, InputEnter, InputEscape, InputState, SelectTrigger,
};
use bolt::primitives::{Badge, BadgeTone, Icon, Kbd, TextVariant, status_diamond};
use bolt::tokens::{Borders, Feedback, Spacing};
use gpui::{
    AnyElement, Context, Div, Entity, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseUpEvent, ParentElement, Role, SharedString, Stateful, Styled, Window,
    anchored, deferred, div, prelude::*,
};
use gpui_kit_assets::IconName;
use reqflux_engine::{SecretScope, VariableScopeRef};

use super::ui;
use crate::shell::variable_popover::{Mode, POPOVER_ID, VariablePopover};
use crate::shell::{Shell, region};
use crate::tokens;

/// The popover, when it is open over `anchor`: an absolute child of the anchor, drawn under
/// it above the islands.
// bolt gap: Popover cannot anchor to a text range inside a field; the anchor is the mark's
// own element (or the field under edit), which the popover hangs under.
pub(crate) fn anchored_at(
    anchor: &str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let popover = shell
        .variable_popover
        .as_ref()
        .filter(|popover| popover.anchor == anchor)?;
    Some(
        div()
            .absolute()
            .top_full()
            .left_0()
            .child(deferred(anchored().snap_to_window().child(frame(popover, cx))).with_priority(3))
            .into_any_element(),
    )
}

fn frame(popover: &VariablePopover, cx: &mut Context<Shell>) -> Stateful<Div> {
    let body = match popover.mode() {
        Mode::Edit => edit(popover, cx),
        Mode::Secret => secret(popover, cx),
        Mode::Add => add(popover, cx),
        Mode::NoEnvironment => no_environment(popover, cx),
    };
    let colors = bolt::colors(cx);
    menu_frame(cx)
        .id(POPOVER_ID)
        .debug_selector(|| POPOVER_ID.to_owned())
        .role(Role::Dialog)
        .aria_label(format!("Variable {}", popover.info.name))
        .track_focus(&popover.focus)
        .occlude()
        .w(tokens::VARIABLE_POPOVER_WIDTH)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .gap(Spacing::SM)
        .text_color(colors.foreground)
        .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
            this.close_variable_popover(window, cx);
        }))
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            cx.stop_propagation();
            if !action.secondary {
                this.save_variable_popover(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            cx.stop_propagation();
            this.escape_variable_popover(window, cx);
        }))
        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                this.escape_variable_popover(window, cx);
            }
        }))
        .children(body)
}

/// The name row: the name in the data face, then `trailing`.
fn header(name: &str, trailing: Vec<AnyElement>, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            ui::role(div(), TextVariant::Code, cx)
                .text_color(colors.strong)
                .child(name.to_owned()),
        )
        .children(trailing)
        .into_any_element()
}

/// The badge of the scope `at`: an environment carries the diamond, the collection reads
/// `Collection · <name>`.
fn scope_badge(at: &VariableScopeRef) -> AnyElement {
    let label = match at.scope {
        SecretScope::Collection(_) => format!("Collection · {}", at.name),
        SecretScope::Global | SecretScope::Environment(_) => at.name.clone(),
    };
    Badge::new(label, BadgeTone::Neutral)
        .id("variable-popover-scope")
        .into_any_element()
}

fn caption(text: impl Into<SharedString>, cx: &Context<Shell>) -> AnyElement {
    ui::role(div(), TextVariant::Caption, cx)
        .child(text.into())
        .into_any_element()
}

/// The value field, element id `variable-popover-input`, with `suffix` controls.
fn field(
    popover: &VariablePopover,
    secret: bool,
    suffix: Vec<AnyElement>,
    cx: &Context<Shell>,
) -> AnyElement {
    let mut input = Input::new(&popover.input)
        .id("variable-popover-input")
        .secret(secret)
        .text_size(TextVariant::Code.density_size(cx))
        .w_full();
    if !suffix.is_empty() {
        input = input.suffix(div().flex().items_center().children(suffix));
    }
    region("variable-popover-field")
        .w_full()
        .child(input)
        .into_any_element()
}

fn icon_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    on_click: impl Fn(&mut Shell, &mut Window, &mut Context<Shell>) + 'static,
    cx: &mut Context<Shell>,
) -> AnyElement {
    Button::new(id, label)
        .ghost()
        .size(ButtonSize::Inline)
        .icon(ui::icon(icon))
        .icon_only()
        .tooltip(label)
        .aria_label(label)
        .on_click(cx.listener(move |this, _event, window, cx| on_click(this, window, cx)))
        .into_any_element()
}

/// The footer: `Enter <verb>`, `Esc close`, then `trailing`.
fn footer(verb: &str, trailing: Vec<AnyElement>, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    div()
        .mt(Spacing::XS)
        .pt(Spacing::SM)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(Kbd::new("Enter"))
        .child(caption(verb.to_owned(), cx))
        .child(Kbd::new("Esc"))
        .child(caption("close", cx))
        .child(div().flex_1())
        .children(trailing)
        .into_any_element()
}

fn open_settings(cx: &mut Context<Shell>) -> AnyElement {
    Button::new("variable-popover-open-settings", "Open in Environments")
        .ghost()
        .size(ButtonSize::Inline)
        .trailing_icon(ui::icon(IconName::ExternalLink))
        .on_click(cx.listener(|this, _event, window, cx| this.open_variable_settings(window, cx)))
        .into_any_element()
}

/// The alert on the name row: `text` with a triangle, in `color`.
fn alert(id: &'static str, text: &'static str, color: Hsla, cx: &Context<Shell>) -> AnyElement {
    region(id)
        .ml_auto()
        .flex()
        .items_center()
        .gap(Spacing::XS)
        .text_color(color)
        .child(
            Icon::new(ui::icon(IconName::TriangleAlert))
                .size(Feedback::STATUS_DIAMOND * 2.0)
                .color(color),
        )
        .child(
            ui::role(div(), TextVariant::BodySm, cx)
                .text_color(color)
                .child(text),
        )
        .into_any_element()
}

/// States a and b: the current value of the winning scope, its initial value and the scope
/// it overrides.
fn edit(popover: &VariablePopover, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let Some(winner) = &popover.info.winner else {
        return Vec::new();
    };
    let copy = icon_button(
        "variable-popover-copy",
        IconName::Copy,
        "Copy value",
        |this, _, cx| {
            this.copy_variable_popover(cx);
        },
        cx,
    );
    let reset = winner.current.is_some().then(|| {
        icon_button(
            "variable-popover-reset",
            IconName::RotateCcw,
            "Reset to initial",
            |this, window, cx| {
                this.reset_variable_popover(window, cx);
            },
            cx,
        )
    });
    let colors = bolt::colors(cx);
    let overrides = popover.info.overrides.as_ref().map(|lower| {
        let scope = match lower.at.scope {
            SecretScope::Collection(_) => format!("collection {}", lower.at.name),
            SecretScope::Environment(_) => format!("environment {}", lower.at.name),
            SecretScope::Global => "Globals".to_owned(),
        };
        let shown = if lower.secret {
            reqflux_engine::MASK.to_owned()
        } else {
            lower.value().unwrap_or_default().to_owned()
        };
        region("variable-popover-overrides")
            .flex()
            .gap(Spacing::XS)
            .child(caption(format!("Overrides {scope}:"), cx))
            .child(
                ui::role(div(), TextVariant::Code, cx)
                    .text_color(colors.muted)
                    .line_through()
                    .child(shown),
            )
            .into_any_element()
    });
    vec![
        header(
            &popover.info.name,
            vec![
                scope_badge(&winner.at),
                div().flex_1().into_any_element(),
                copy,
            ],
            cx,
        ),
        caption("Current value · this machine", cx),
        field(popover, false, reset.into_iter().collect(), cx),
        caption("Initial value · workspace file", cx),
        ui::role(region("variable-popover-initial"), TextVariant::Code, cx)
            .text_color(colors.muted)
            .child(winner.initial.clone().unwrap_or_default())
            .into_any_element(),
    ]
    .into_iter()
    .chain(overrides)
    .chain([footer("save", vec![open_settings(cx)], cx)])
    .collect()
}

/// State c: a secret, masked until the eye reads its keyring value.
fn secret(popover: &VariablePopover, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let Some(winner) = &popover.info.winner else {
        return Vec::new();
    };
    let reveal = (!popover.revealed).then(|| {
        icon_button(
            "variable-popover-reveal",
            IconName::Eye,
            "Reveal value",
            |this, window, cx| {
                this.reveal_variable_popover(window, cx);
            },
            cx,
        )
    });
    vec![
        header(
            &popover.info.name,
            vec![scope_badge(&winner.at), super::secret().into_any_element()],
            cx,
        ),
        caption("Value · OS keyring", cx),
        field(popover, true, reveal.into_iter().collect(), cx),
        caption(
            "Never written to the workspace file, logs or MCP responses.",
            cx,
        ),
        footer("save to keyring", vec![open_settings(cx)], cx),
    ]
}

/// The label of the quick-add scope `target`: an environment by name, marked when it is the
/// active one, the collection as `Collection · <name>`, or `Globals`.
fn target_label(target: &VariableScopeRef, popover: &VariablePopover) -> String {
    match target.scope {
        SecretScope::Collection(_) => format!("Collection · {}", target.name),
        SecretScope::Global => target.name.clone(),
        SecretScope::Environment(_) if popover.info.active_target == Some(target.scope) => {
            format!("{} · active", target.name)
        }
        SecretScope::Environment(_) => target.name.clone(),
    }
}

/// The "Add to" dropdown: `bolt`'s select trigger, element id `variable-popover-target`, and,
/// while open, its list of rows `variable-popover-target-<n>` under it. The list is part of the
/// popover, not a layer over it, so a click on a row is a click inside the popover.
// bolt gap: Dropdown draws its list in a layer above the window; the popover treats a
// click outside its own frame as a dismissal, so the list stays in the frame.
fn target_dropdown(popover: &VariablePopover, cx: &mut Context<Shell>) -> AnyElement {
    let tint = bolt::brand::brand(cx).accent_tint;
    let label = popover
        .info
        .targets
        .get(popover.target)
        .map(|target| target_label(target, popover))
        .unwrap_or_default();
    let trigger = SelectTrigger::new("variable-popover-target", label.clone())
        .aria_label(format!("Add to: {label}"))
        .focus_ring(popover.target_menu.then_some(tint))
        .on_click(cx.listener(|this, _event, _window, cx| {
            this.toggle_variable_target_menu(cx);
        }));
    let rows = popover
        .info
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let name = target_label(target, popover);
            let selected = index == popover.target;
            let id = format!("variable-popover-target-{}", index + 1);
            let selector = id.clone();
            menu_row(
                id,
                &MenuItem::new(name.clone()).checked(selected),
                selected,
                cx,
            )
            .debug_selector(move || selector)
            .aria_label(name)
            .aria_selected(selected)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.pick_variable_target(index, cx);
            }))
            .into_any_element()
        });
    let list = popover.target_menu.then(|| {
        menu_frame(cx)
            .id("variable-popover-target-list")
            .role(Role::Menu)
            .max_h(tokens::VARIABLE_POPOVER_TARGET_LIST_HEIGHT)
            .overflow_y_scroll()
            .children(rows.collect::<Vec<_>>())
    });
    div()
        .flex()
        .flex_col()
        .gap(Spacing::XS)
        .w_full()
        .child(trigger)
        .children(list)
        .into_any_element()
}

/// State d: not defined; the "Add to" dropdown, the value and the Secret box.
fn add(popover: &VariablePopover, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let picker = target_dropdown(popover, cx);
    let secret_box = Checkbox::new("variable-popover-secret")
        .checked(popover.secret)
        .label("Secret (store in OS keyring)")
        .on_click(
            cx.listener(|this, _checked: &bool, window, cx| this.toggle_popover_secret(window, cx)),
        );
    let buttons = vec![
        Button::new("variable-popover-cancel", "Cancel")
            .secondary()
            .size(ButtonSize::Inline)
            .on_click(
                cx.listener(|this, _event, window, cx| this.close_variable_popover(window, cx)),
            )
            .into_any_element(),
        Button::new("variable-popover-add", "Add variable")
            .primary()
            .size(ButtonSize::Inline)
            .on_click(
                cx.listener(|this, _event, window, cx| this.save_variable_popover(window, cx)),
            )
            .into_any_element(),
    ];
    vec![
        header(
            &popover.info.name,
            vec![alert(
                "variable-popover-not-defined",
                "Not defined",
                bolt::colors(cx).danger,
                cx,
            )],
            cx,
        ),
        caption("Add to", cx),
        picker,
        caption("Value", cx),
        field(popover, popover.secret, Vec::new(), cx),
        secret_box.into_any_element(),
        footer("add", buttons, cx),
    ]
}

/// State e: no active environment, and only environments declare the name.
fn no_environment(popover: &VariablePopover, cx: &mut Context<Shell>) -> Vec<AnyElement> {
    let tint = bolt::brand::brand(cx).accent_tint;
    let environments: Vec<AnyElement> = popover
        .info
        .environments
        .iter()
        .map(|choice| {
            let slug = choice.slug.clone();
            div()
                .flex()
                .items_center()
                .gap(Spacing::XS)
                .child(status_diamond(tint, Feedback::STATUS_DIAMOND))
                .child(
                    Button::new(
                        SharedString::from(format!("variable-popover-environment-{slug}")),
                        choice.at.name.clone(),
                    )
                    .secondary()
                    .size(ButtonSize::Inline)
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.activate_variable_environment(&slug, window, cx);
                    })),
                )
                .into_any_element()
        })
        .collect();
    let instead = Button::new(
        "variable-popover-add-instead",
        "Add to collection or globals instead",
    )
    .ghost()
    .size(ButtonSize::Inline)
    .on_click(cx.listener(|this, _event, window, cx| this.add_variable_instead(window, cx)))
    .into_any_element();
    let colors = bolt::colors(cx);
    vec![
        header(
            &popover.info.name,
            vec![alert(
                "variable-popover-no-environment",
                "No active environment",
                bolt::colors(cx).warning,
                cx,
            )],
            cx,
        ),
        ui::role(div(), TextVariant::BodySm, cx)
            .text_color(colors.foreground)
            .child("Defined only in environments:")
            .into_any_element(),
        div()
            .flex()
            .flex_wrap()
            .gap(Spacing::SM)
            .children(environments)
            .into_any_element(),
        caption("Pick one to activate it for this workspace.", cx),
        div()
            .mt(Spacing::XS)
            .pt(Spacing::SM)
            .border_t(Borders::THIN)
            .border_color(colors.line)
            .flex()
            .items_center()
            .gap(Spacing::SM)
            .child(instead)
            .child(div().flex_1())
            .child(open_settings(cx))
            .into_any_element(),
    ]
}

/// `field`, the field `anchor` of the request `request_id` under edit over `input`, with
/// the editing keys of the popover: F2, or Alt+click, on a reference opens its popover under
/// the field.
pub(crate) fn editing_keys(
    field: Stateful<Div>,
    request_id: &str,
    anchor: &str,
    input: &Entity<InputState>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let popover = anchored_at(anchor, shell, cx);
    let (key_request, key_anchor, key_input) =
        (request_id.to_owned(), anchor.to_owned(), input.clone());
    let (click_request, click_anchor, click_input) =
        (request_id.to_owned(), anchor.to_owned(), input.clone());
    field
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "f2" {
                cx.stop_propagation();
                this.open_variable_popover_at(&key_request, &key_anchor, &key_input, window, cx);
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                if event.modifiers.alt {
                    this.open_variable_popover_at(
                        &click_request,
                        &click_anchor,
                        &click_input,
                        window,
                        cx,
                    );
                }
            }),
        )
        .children(popover)
}
