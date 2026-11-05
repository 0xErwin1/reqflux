//! The variable popover as design/variable-popover/01-inline-popover.html draws it, on
//! `bolt`'s form popover: the name with the winning scope badge, the value field, the initial
//! value and the scope it overrides, and a footer with the Enter and Esc keycaps and
//! Open in Environments. A secret shows its SECRET badge and a reveal eye; an undefined name
//! the quick add (the "Add to" dropdown, value, Secret box); with no active environment, the
//! environments that declare it.

use bolt::composites::Popover;
use bolt::controls::{Button, ButtonSize, Checkbox, Input, InputEnter, InputEscape, InputState};
use bolt::primitives::{Badge, BadgeTone, Icon, Kbd, TextVariant, status_diamond};
use bolt::tokens::{Feedback, PopoverMetrics, Spacing};
use gpui::{
    AnyElement, Bounds, Context, Div, Entity, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseUpEvent, ParentElement, Pixels, SharedString, Stateful, Styled, Window, div,
    size,
};
use gpui_kit_assets::IconName;
use reqflux_engine::{SecretScope, VariableScopeRef};

use super::ui;
use crate::shell::variable_popover::{Mode, POPOVER_ID, VariablePopover};
use crate::shell::{Shell, region};
use crate::tokens;

/// The rectangle of the element `anchor` in window coordinates, recorded as it is laid out,
/// and, while the popover over it is open, the surface itself: `bolt`'s form popover, placed
/// under that rectangle, flipping above it when there is no room below.
pub(crate) fn anchored_at(
    anchor: &str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Option<AnyElement> {
    let tracker = tracker(anchor, shell);
    let surface = shell
        .variable_popover
        .as_ref()
        .filter(|popover| popover.anchor == anchor)
        .map(|popover| surface(popover, cx));
    let placed = div().absolute().top_full().left_0().child(tracker);
    Some(match surface {
        Some(surface) => placed.child(surface).into_any_element(),
        None => placed.into_any_element(),
    })
}

/// Records the origin of the element it is laid out in, so the surface opens under it: the
/// mark, or the field under edit. The rectangle carries the frame that drew it, so the shell
/// drops the ones no mark drew any more.
fn tracker(anchor: &str, shell: &Shell) -> AnyElement {
    let anchors = shell.variable_anchors.clone();
    let key = anchor.to_owned();
    gpui::canvas(
        move |bounds, _window, _cx| {
            let nothing = size(Pixels::ZERO, Pixels::ZERO);
            anchors.record(&key, Bounds::new(bounds.origin, nothing));
        },
        |_, _, _, _| {},
    )
    .into_any_element()
}

/// `bolt`'s surface over `popover`, with the parts of its mode.
fn surface(popover: &VariablePopover, cx: &mut Context<Shell>) -> AnyElement {
    let (body, footer) = match popover.mode() {
        Mode::Edit => edit(popover, cx),
        Mode::Secret => secret(popover, cx),
        Mode::Add => add(popover, cx),
        Mode::NoEnvironment => no_environment(popover, cx),
    };
    let body = div()
        .flex()
        .flex_col()
        .gap(PopoverMetrics::GAP)
        .on_action(cx.listener(|this, action: &InputEnter, window, cx| {
            cx.stop_propagation();
            if !action.secondary {
                this.save_variable_popover(window, cx);
            }
        }))
        // A press inside the surface keeps the focus where it was, in the value field: the
        // surface would take it, and Escape is the input's key, not a bare surface's.
        .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
            window.prevent_default()
        })
        .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
            // ReqFlux installs no `bolt` `CommandSet` and binds Escape to `InputEscape` in its
            // fields, so that binding consumes the key before the surface sees it and the body
            // answers the action instead, closing the "Add to" list first and itself next.
            cx.stop_propagation();
            this.escape_variable_popover(window, cx);
        }))
        .children(body);
    Popover::new(POPOVER_ID, popover.state.clone())
        .aria_label(format!("Variable {}", popover.info.name))
        .body(body)
        .footer(footer)
        .width(tokens::VARIABLE_POPOVER_WIDTH)
        .into_any_element()
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
    div()
        .w_full()
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
fn edit(popover: &VariablePopover, cx: &mut Context<Shell>) -> (Vec<AnyElement>, Vec<AnyElement>) {
    let Some(winner) = &popover.info.winner else {
        return (Vec::new(), Vec::new());
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
    let body = vec![
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
    .collect();
    (body, vec![footer("save", vec![open_settings(cx)], cx)])
}

/// State c: a secret, masked until the eye reads its keyring value.
fn secret(
    popover: &VariablePopover,
    cx: &mut Context<Shell>,
) -> (Vec<AnyElement>, Vec<AnyElement>) {
    let Some(winner) = &popover.info.winner else {
        return (Vec::new(), Vec::new());
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
    let body = vec![
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
    ];
    (
        body,
        vec![footer("save to keyring", vec![open_settings(cx)], cx)],
    )
}

/// The "Add to" dropdown, element id `variable-popover-target`: `bolt`'s `Dropdown` over every
/// scope a quick add can pick, its selection the scope. Its menu is a layer `bolt` draws above
/// the surface, so a click on a row is a click inside the popover; the rows carry `bolt`'s ids
/// (`variable-popover-target-item-<n>`, numbered from 0), so a test picks a scope by name.
fn target_dropdown(popover: &VariablePopover) -> AnyElement {
    div()
        .w_full()
        .child(popover.target_select.clone())
        .into_any_element()
}

/// State d: not defined; the "Add to" dropdown, the value and the Secret box.
fn add(popover: &VariablePopover, cx: &mut Context<Shell>) -> (Vec<AnyElement>, Vec<AnyElement>) {
    let picker = target_dropdown(popover);
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
    let body = vec![
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
    ];
    (body, vec![footer("add", buttons, cx)])
}

/// State e: no active environment, and only environments declare the name.
fn no_environment(
    popover: &VariablePopover,
    cx: &mut Context<Shell>,
) -> (Vec<AnyElement>, Vec<AnyElement>) {
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
    let body = vec![
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
    ];
    let footer = div()
        .w_full()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(instead)
        .child(div().flex_1())
        .child(open_settings(cx))
        .into_any_element();
    (body, vec![footer])
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
