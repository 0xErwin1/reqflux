//! The sidebar filter field, `tree-filter`: `bolt`'s field with the search icon and the `/`
//! keycap. Typing narrows the tree (see [`TreeState::set_filter`]), `/` in the tree focuses
//! the field, Escape in it clears it, and a filter that matches nothing shows the caption
//! `tree-no-match`. As drawn in design/bolt-v2/01-main.html, States.

use bolt::controls::{Input, InputEscape, InputEvent, InputState};
use bolt::primitives::{Chamfer, ChamferRing, Icon, TextVariant};
use bolt::tokens::{ChamferCut, Fields, HeaderMetrics, ShellMetrics};
use gpui::{
    App, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, Styled,
    Subscription, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::{Sidebar, keycap};
use crate::shell::request::ui;
use crate::shell::tree_model::TreeState;
use crate::shell::{region, text_region};

/// The placeholder of the filter field.
pub(crate) const FILTER_PLACEHOLDER: &str = "Filter requests";
/// Shown when the filter matches no request.
pub(crate) const NO_MATCH: &str = "No requests match the filter";

/// The filter's text field and its change subscription.
pub(crate) struct TreeFilter {
    input: Entity<InputState>,
    _change: Subscription,
}

impl Sidebar {
    /// The filter's text field, made on first use.
    pub(super) fn filter_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(filter) = &self.filter {
            return filter.input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(FILTER_PLACEHOLDER));
        let change = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.update_tree(cx, |tree| tree.set_filter(&query));
            }
        });
        self.filter = Some(TreeFilter {
            input: input.clone(),
            _change: change,
        });
        input
    }

    /// `/`: moves the keyboard focus to the filter field.
    pub(super) fn focus_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.filter_input(window, cx);
        input.update(cx, |state, cx| state.focus(window, cx));
    }

    /// Escape in the field: clears it, the whole tree shows again and takes the keyboard
    /// focus back.
    fn clear_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.filter_input(window, cx);
        input.update(cx, |state, cx| state.set_value("", window, cx));
        self.update_tree(cx, |tree| tree.set_filter(""));
        self.focus.focus(window, cx);
    }

    /// The filter field, drawn as DBFlux's sidebar filter: a 30 px chamfered frame
    /// `tree-filter` with the focus ring while the field has focus, the search icon, the
    /// frameless text field `tree-filter-input` and, while it is empty, the `/` keycap.
    pub(super) fn filter_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let input = self.filter_input(window, cx);
        let empty = input.read(cx).value().is_empty();
        let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
        let colors = bolt::colors(cx);
        let mut shape = Chamfer::new(ChamferCut::CONTROL)
            .fill(colors.background)
            .border(colors.line);
        if focused {
            shape = shape.ring(ChamferRing::focus(bolt::brand::brand(cx).accent_tint));
        }
        div()
            .flex_none()
            .pl(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
            .pr(ShellMetrics::SIDEBAR_FILTER_PADDING_X)
            .pb(ShellMetrics::SIDEBAR_FILTER_PADDING_BOTTOM)
            .font_family(bolt::fonts::ui_family(cx))
            .child(
                region("tree-filter")
                    .relative()
                    .flex()
                    .items_center()
                    .gap(Fields::GAP)
                    .h(Fields::HEIGHT)
                    .pl(Fields::PADDING_X)
                    .pr(Fields::PADDING_X)
                    .text_size(Fields::TEXT)
                    .capture_action(cx.listener(|this, _: &InputEscape, window, cx| {
                        this.clear_filter(window, cx);
                        cx.stop_propagation();
                    }))
                    .child(shape)
                    .child(
                        Icon::new(ui::icon(IconName::Search))
                            .size(ShellMetrics::SIDEBAR_FILTER_ICON)
                            .color(colors.muted),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&input)
                                .id("tree-filter-input")
                                .aria_label(FILTER_PLACEHOLDER)
                                .small()
                                .appearance(false),
                        ),
                    )
                    .when(empty, |field| {
                        field.child(keycap("tree-filter-keycap", "/"))
                    }),
            )
    }
}

/// The caption `tree-no-match` that takes the rows' place when the filter matches nothing.
pub(super) fn no_match(tree: &TreeState, cx: &App) -> Option<impl IntoElement + use<>> {
    let (requests, _) = tree.shown_counts();
    (tree.is_filtered() && requests == 0).then(|| {
        ui::role(text_region("tree-no-match"), TextVariant::Caption, cx)
            .aria_label(NO_MATCH)
            .pl(HeaderMetrics::PANEL_PADDING_LEFT)
            .pr(HeaderMetrics::PANEL_PADDING_LEFT)
            .child(NO_MATCH)
    })
}
