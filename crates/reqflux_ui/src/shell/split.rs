//! The split between the request and its response (request-pane-feedback S6, artboard
//! request-panes/01-stacked-grip): the request pane on top, `bolt`'s splitter (a 6 px gutter
//! with a dotted grip), element id `request-response-split`, and the response under it. The
//! panes are `request-area` and `response-area`.
//!
//! In the Side by side layout (v1-features S7, design/layouts/01) the same split stands
//! vertically between the request and response columns: `bolt`'s splitter drawn with
//! [`SplitAxis::Horizontal`], the request column's width kept instead of its height.
//!
//! Dragging the gutter sets the request pane's height or the request column's width; the
//! arrows along the axis move it by `SplitterMetrics::KEY_STEP` while the gutter has focus.
//! Enter or a double click collapses the response to its bar, element id `response-bar`,
//! across the island in both modes, and restores it; the bar's expand button,
//! `response-expand`, and any drag restore it too. Each pane keeps at least
//! [`tokens::SPLIT_PANE_MIN`]. The position and the collapsed flag are per user: the app
//! state settings [`HEIGHT_SETTING`], [`WIDTH_SETTING`] and [`COLLAPSED_SETTING`] hold them
//! across restarts. The layout mode itself is `layout` in the user's config file (see
//! `layout`).
//!
//! A request not sent in this session shows its last recorded send from history; one never
//! sent shows neither the gutter nor the response: its pane takes the whole height
//! (request-pane-feedback S12, frame D). The first send brings both in at the stored
//! position. Document mode (v1-features S7) draws no split at all: the document island
//! shows the active request's folder as a read-only `.http` document (see `document_view`),
//! and the response is not shown.

use std::collections::HashSet;

use bolt::Colors;
use bolt::composites::{SplitAxis, Splitter};
use bolt::controls::Button;
use bolt::primitives::Icon;
use bolt::tokens::{
    Borders, ButtonMetrics, Fields, HeaderMetrics, ResultMetrics, Spacing, SplitterMetrics,
};
use gpui::{
    AnyElement, Context, Div, FocusHandle, FontWeight, Hsla, MouseButton, ParentElement, Pixels,
    Stateful, Styled, canvas, div, prelude::*, relative,
};
use gpui_kit_assets::IconName;
use reqflux_engine::config::LayoutMode;
use reqflux_engine::state::AppState;

use super::request::{Completed, SendState, StatusClass, elapsed_text, size_text, ui};
use super::send::RequestSend;
use super::{Shell, region, response};
use crate::tokens;

/// The app state setting that holds the request pane's height in pixels.
pub(super) const HEIGHT_SETTING: &str = "request_response_split/height";
/// The app state setting that holds the request column's width in pixels (Side by side).
pub(super) const WIDTH_SETTING: &str = "request_response_split/width";
/// The app state setting that holds `true` while the response is collapsed.
pub(super) const COLLAPSED_SETTING: &str = "request_response_split/collapsed";

/// The share of the split's width the request column takes before the user moves it, as
/// the artboard draws the columns (design/layouts/01).
const DEFAULT_REQUEST_SHARE: f32 = 0.46;

/// The position of the split, shared by every request tab.
pub(super) struct Split {
    /// The layout mode the split draws (layouts S7); changed through `layout`.
    pub(super) mode: LayoutMode,
    /// The request pane's height as last set, before it is fitted to the split's height.
    request: Pixels,
    /// The request column's width as last set, `None` for the default share (Side by side).
    request_w: Option<Pixels>,
    collapsed: bool,
    /// The split's height at the last layout; `None` before the first one.
    height: Option<Pixels>,
    /// The split's width at the last layout; `None` before the first one.
    width: Option<Pixels>,
    /// The split's top in the window at the last layout.
    top: Pixels,
    /// The split's left in the window at the last layout.
    left: Pixels,
    dragging: bool,
    focus: FocusHandle,
    /// The requests whose last recorded send was looked up, so a never-sent request reads
    /// the history once, not on every frame; recording a send clears it.
    recorded: HashSet<String>,
}

impl Split {
    pub(super) fn new(focus: FocusHandle) -> Self {
        Self {
            mode: LayoutMode::default(),
            request: tokens::SPLIT_DEFAULT_REQUEST,
            request_w: None,
            collapsed: false,
            height: None,
            width: None,
            top: Pixels::ZERO,
            left: Pixels::ZERO,
            dragging: false,
            focus,
            recorded: HashSet::new(),
        }
    }

    /// Marks the last recorded send of `id` as looked up; `true` when it was not yet.
    pub(super) fn mark_recorded(&mut self, id: &str) -> bool {
        self.recorded.insert(id.to_owned())
    }

    /// Looks the last recorded sends up again, after history changed.
    pub(super) fn forget_recorded(&mut self) {
        self.recorded.clear();
    }

    /// Takes the position stored in `state`, keeping the default for what is missing or
    /// unreadable.
    pub(super) fn restore(&mut self, state: Option<&AppState>) {
        let Some(state) = state else {
            return;
        };
        match state.setting(HEIGHT_SETTING) {
            Ok(Some(stored)) => match parse_pixels(&stored) {
                Some(height) => self.request = height,
                None => log::warn!("ignoring the stored request pane height {stored:?}"),
            },
            Ok(None) => {}
            Err(error) => log::warn!("cannot read the request pane height: {error}"),
        }
        match state.setting(WIDTH_SETTING) {
            Ok(Some(stored)) => match parse_pixels(&stored) {
                Some(width) => self.request_w = Some(width),
                None => log::warn!("ignoring the stored request column width {stored:?}"),
            },
            Ok(None) => {}
            Err(error) => log::warn!("cannot read the request column width: {error}"),
        }
        match state.setting(COLLAPSED_SETTING) {
            Ok(stored) => self.collapsed = stored.as_deref() == Some("true"),
            Err(error) => log::warn!("cannot read whether the response is collapsed: {error}"),
        }
    }

    /// Stores the position in `state`.
    fn save(&self, state: Option<&mut AppState>) {
        let Some(state) = state else {
            return;
        };
        let height = format!("{}", f32::from(self.request).round());
        if let Err(error) = state.set_setting(HEIGHT_SETTING, &height) {
            log::warn!("cannot save the request pane height: {error}");
        }
        if let Some(width) = self.request_w {
            let width = format!("{}", f32::from(width).round());
            if let Err(error) = state.set_setting(WIDTH_SETTING, &width) {
                log::warn!("cannot save the request column width: {error}");
            }
        }
        let collapsed = if self.collapsed { "true" } else { "false" };
        if let Err(error) = state.set_setting(COLLAPSED_SETTING, collapsed) {
            log::warn!("cannot save whether the response is collapsed: {error}");
        }
    }

    #[cfg(test)]
    pub(super) fn collapsed(&self) -> bool {
        self.collapsed
    }

    /// The request pane's height inside a split `height` tall: the stored height, kept
    /// [`tokens::SPLIT_PANE_MIN`] away from both ends. A split too short for both minimums
    /// shares its room evenly.
    pub(super) fn request_height(&self, height: Pixels) -> Pixels {
        fit(self.request, height)
    }

    /// The axis the split lays its panes out in: columns side by side while the layout is
    /// Side by side and the response is shown, else stacked rows. Document mode draws no
    /// split; the document island shows the folder's `.http` text (`document_view`)
    /// instead.
    fn axis(&self) -> SplitAxis {
        match self.mode {
            LayoutMode::SideBySide if !self.collapsed => SplitAxis::Horizontal,
            LayoutMode::SideBySide | LayoutMode::Stacked | LayoutMode::Document => {
                SplitAxis::Vertical
            }
        }
    }

    /// The request column's width inside a split `width` wide: the stored width, else the
    /// default share the artboard draws (design/layouts/01), kept
    /// [`tokens::SPLIT_PANE_MIN`] away from both ends. A split too narrow for both
    /// minimums shares its room evenly.
    fn request_width(&self, width: Pixels) -> Pixels {
        let stored = self
            .request_w
            .unwrap_or_else(|| width * DEFAULT_REQUEST_SHARE);
        fit(stored, width)
    }

    /// Sets the request pane's height, fitted to the last layout, and shows the response.
    fn set_request(&mut self, request: Pixels) {
        self.request = match self.height {
            Some(height) => fit(request, height),
            None => request.max(tokens::SPLIT_PANE_MIN),
        };
        self.collapsed = false;
    }

    /// Sets the request column's width, fitted to the last layout, and shows the response.
    fn set_request_width(&mut self, request: Pixels) {
        self.request_w = Some(match self.width {
            Some(width) => fit(request, width),
            None => request.max(tokens::SPLIT_PANE_MIN),
        });
        self.collapsed = false;
    }

    /// Moves the gutter by `delta` along its axis.
    fn step(&mut self, delta: Pixels) {
        match self.axis() {
            SplitAxis::Horizontal => {
                let shown = match self.width {
                    Some(width) => self.request_width(width),
                    None => match self.request_w {
                        Some(width) => width,
                        None => return,
                    },
                };
                self.set_request_width(shown + delta);
            }
            SplitAxis::Vertical => {
                let shown = match self.height {
                    Some(height) => self.request_height(height),
                    None => self.request,
                };
                self.set_request(shown + delta);
            }
        }
    }
}

/// `request` kept [`tokens::SPLIT_PANE_MIN`] away from both ends of a split `height` or
/// `width` tall.
fn fit(request: Pixels, span: Pixels) -> Pixels {
    let room = (span - SplitterMetrics::GUTTER).max(Pixels::ZERO);
    let max = room - tokens::SPLIT_PANE_MIN;
    if max < tokens::SPLIT_PANE_MIN {
        return room * 0.5;
    }
    request.clamp(tokens::SPLIT_PANE_MIN, max)
}

/// A stored position: a finite, positive number of pixels.
fn parse_pixels(stored: &str) -> Option<Pixels> {
    stored
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|height| height.is_finite() && *height > 0.0)
        .map(Pixels::from)
}

impl Shell {
    /// Moves the gutter by `delta` and stores the position.
    pub(super) fn step_split(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        self.split.step(delta);
        self.split.save(self.state.as_mut());
        cx.notify();
    }

    /// Collapses the response, or restores it, and stores the position.
    pub(super) fn toggle_response(&mut self, cx: &mut Context<Self>) {
        self.split.collapsed = !self.split.collapsed;
        self.split.save(self.state.as_mut());
        cx.notify();
    }

    /// Restores the collapsed response and stores the position.
    fn expand_response(&mut self, cx: &mut Context<Self>) {
        self.split.collapsed = false;
        self.split.save(self.state.as_mut());
        cx.notify();
    }

    /// Follows a drag of the gutter to `position` along its axis, in window coordinates.
    fn drag_split(&mut self, position: Pixels, cx: &mut Context<Self>) {
        self.split.dragging = true;
        match self.split.axis() {
            SplitAxis::Horizontal => {
                let x = position - self.split.left;
                self.split
                    .set_request_width(x - SplitterMetrics::GUTTER * 0.5);
            }
            SplitAxis::Vertical => {
                let y = position - self.split.top;
                self.split.set_request(y - SplitterMetrics::GUTTER * 0.5);
            }
        }
        cx.notify();
    }

    /// Ends a drag of the gutter and stores where it ended.
    fn end_split_drag(&mut self, cx: &mut Context<Self>) {
        if !self.split.dragging {
            return;
        }
        self.split.dragging = false;
        self.split.save(self.state.as_mut());
        cx.notify();
    }
}

/// The split of the request `id`: its `request` parts in the request pane, the gutter, and
/// the response of `send` or, while collapsed, its bar. Without a `send` the request pane
/// fills the split alone.
pub(super) fn render(
    id: &str,
    request: impl IntoIterator<Item = impl IntoElement>,
    send: Option<&RequestSend>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    let split = &shell.split;
    let entity = cx.entity();
    let measure = canvas(
        move |bounds, _window, cx| {
            entity.update(cx, |shell, _cx| {
                shell.split.top = bounds.top();
                shell.split.left = bounds.left();
                shell.split.height = Some(bounds.size.height);
                shell.split.width = Some(bounds.size.width);
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();

    let request_pane = region("request-area")
        .flex()
        .flex_col()
        .min_w_0()
        .overflow_hidden()
        .children(request);
    let Some(send) = send else {
        if !split.recorded.contains(id) {
            let id = id.to_owned();
            let shell = cx.entity();
            cx.defer(move |cx| {
                shell.update(cx, |shell, cx| {
                    shell.split.recorded.insert(id.clone());
                    shell.seed_recorded_send(&id, cx);
                });
            });
        }
        return region("request-response-area")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(request_pane.flex_1().min_h_0());
    };
    // Collapsed, the request fills the island and the response is its bar across it, in
    // both modes (design/layouts/01, narrow-window state D9).
    let request_pane = if split.collapsed {
        request_pane.flex_1().min_h_0()
    } else {
        match split.mode {
            LayoutMode::Stacked | LayoutMode::Document => match split.height {
                Some(height) => request_pane.flex_none().h(split.request_height(height)),
                None => request_pane
                    .h(split.request)
                    .flex_grow(0.)
                    .flex_shrink(1.)
                    .min_h(tokens::SPLIT_PANE_MIN),
            },
            LayoutMode::SideBySide => {
                let request_pane = match split.width {
                    Some(width) => request_pane.flex_none().w(split.request_width(width)),
                    None => request_pane.flex_none().w(relative(DEFAULT_REQUEST_SHARE)),
                };
                request_pane.min_w(tokens::SPLIT_PANE_MIN)
            }
        }
    };
    let response: AnyElement = if split.collapsed {
        response_bar(Some(send), cx).into_any_element()
    } else {
        region("response-area")
            .flex_1()
            .min_h(tokens::SPLIT_PANE_MIN)
            .min_w(tokens::SPLIT_PANE_MIN)
            .flex()
            .flex_col()
            .child(response::render(id, Some(send), cx))
            .into_any_element()
    };

    let area = region("request-response-area")
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex();
    let area = match split.axis() {
        SplitAxis::Horizontal => area.flex_row(),
        SplitAxis::Vertical => area.flex_col(),
    };
    area.child(measure)
        .child(request_pane)
        .child(gutter(shell, cx))
        .child(response)
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| this.end_split_drag(cx)),
        )
}

/// The gutter, element id `request-response-split`: `bolt`'s splitter along the split's
/// axis, named "Resize request and response", its value the request pane's share once
/// laid out.
fn gutter(shell: &Shell, cx: &mut Context<Shell>) -> Splitter {
    let split = &shell.split;
    let axis = split.axis();
    let percent = match axis {
        SplitAxis::Horizontal => split
            .width
            .filter(|width| *width > Pixels::ZERO)
            .map(|width| f64::from((split.request_width(width) / width * 100.0).round())),
        SplitAxis::Vertical => split
            .height
            .filter(|height| *height > Pixels::ZERO)
            .map(|height| {
                let request = if split.collapsed {
                    height
                } else {
                    split.request_height(height)
                };
                f64::from((request / height * 100.0).round())
            }),
    };
    let (on_drag, on_step, on_toggle) = (cx.entity(), cx.entity(), cx.entity());
    let splitter =
        Splitter::new("request-response-split", axis).aria_label("Resize request and response");
    let splitter = match percent {
        Some(percent) => splitter.value_percent(percent),
        None => splitter,
    };
    splitter
        .active(split.dragging)
        .track_focus(&split.focus)
        .on_drag(move |position, _window, cx| {
            on_drag.update(cx, |shell, cx| shell.drag_split(position, cx));
        })
        .on_step(move |delta, _window, cx| {
            on_step.update(cx, |shell, cx| shell.step_split(delta, cx));
        })
        .on_toggle_collapse(move |_window, cx| {
            on_toggle.update(cx, |shell, cx| shell.toggle_response(cx));
        })
}

/// The collapsed response, element id `response-bar`: the expand button, the response's
/// status, time and size and its test count, and how to bring it back.
fn response_bar(send: Option<&RequestSend>, cx: &mut Context<Shell>) -> Stateful<Div> {
    let expand = expand_button(cx);
    let colors = bolt::colors(cx);
    let mono_family = bolt::fonts::editor_family(cx);
    let notice_size = bolt::density::font_xs(cx);
    let mono = |id: &'static str, label: String, color: Hsla| {
        super::text_region(id)
            .aria_label(label.clone())
            .font_family(mono_family.clone())
            .text_size(ResultMetrics::STATEMENT_FONT)
            .text_color(color)
            .whitespace_nowrap()
            .child(label)
    };
    let mut parts: Vec<AnyElement> = Vec::new();
    match send.map(|send| &send.state) {
        Some(SendState::Sending { .. }) => {
            parts.push(
                mono("response-bar-status", "Sending".to_owned(), colors.muted).into_any_element(),
            );
        }
        Some(SendState::Done(Completed::Response(received))) => {
            let status = class_color(StatusClass::of(received.status), &colors);
            parts.push(
                mono("response-bar-status", received.status_text(), status)
                    .font_weight(FontWeight::SEMIBOLD)
                    .into_any_element(),
            );
            let meta = format!(
                "{} · {}",
                elapsed_text(received.elapsed),
                size_text(received.size())
            );
            parts.push(mono("response-bar-meta", meta, colors.muted).into_any_element());
            let tests = &received.scripts.tests;
            if !tests.is_empty() {
                let passed = tests.iter().filter(|test| test.passed).count();
                let failed = tests.len() - passed;
                let (label, color) = match failed {
                    0 => (format!("{passed} passed"), colors.success),
                    _ => (format!("{failed} failed"), colors.danger),
                };
                parts.push(
                    super::text_region("response-bar-tests")
                        .aria_label(label.clone())
                        .flex()
                        .items_center()
                        .gap(Spacing::XXS)
                        .text_size(notice_size)
                        .text_color(color)
                        .child(
                            Icon::new(ui::icon(IconName::CircleCheckBig))
                                .size(Fields::CHEVRON)
                                .color(color),
                        )
                        .child(label)
                        .into_any_element(),
                );
            }
        }
        Some(SendState::Done(Completed::Failure(_))) => {
            parts.push(
                mono("response-bar-status", "Failed".to_owned(), colors.danger)
                    .font_weight(FontWeight::SEMIBOLD)
                    .into_any_element(),
            );
        }
        None | Some(SendState::Idle) => {}
    }

    region("response-bar")
        .flex_none()
        .h(HeaderMetrics::PANEL_HEIGHT)
        .flex()
        .items_center()
        .gap(HeaderMetrics::BAR_GAP)
        .pl(Spacing::MD)
        .pr(Spacing::SM)
        .border_t(Borders::THIN)
        .border_color(colors.line_2)
        .text_size(ButtonMetrics::FONT)
        .child(expand)
        .child(
            div()
                .text_color(colors.strong)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Response"),
        )
        .children(parts)
        .child(div().flex_1())
        .child(
            div()
                .text_size(notice_size)
                .text_color(colors.muted)
                .whitespace_nowrap()
                .child("Collapsed · drag the handle"),
        )
}

/// The ghost button that restores the collapsed response, element id `response-expand`.
fn expand_button(cx: &mut Context<Shell>) -> Button {
    Button::new("response-expand", "Expand response")
        .ghost()
        .icon_only()
        .icon(ui::icon(IconName::ChevronUp))
        .on_click(cx.listener(|this, _event, _window, cx| this.expand_response(cx)))
}

/// The status color of a status class, as the response's status chip draws it.
fn class_color(class: StatusClass, colors: &Colors) -> Hsla {
    match class {
        StatusClass::Success => colors.success,
        StatusClass::Info => colors.info,
        StatusClass::Warning => colors.warning,
        StatusClass::Danger => colors.danger,
    }
}
