//! The run document's chrome: its tab chip, the header with the verdict and totals, the
//! progress bar, and the collection settings' `Run…` button. The setup, grid and result are
//! their own modules.

use bolt::composites::{document_tab, document_tab_title};
use bolt::controls::Button;
use bolt::primitives::{Badge, BadgeTone, Icon, Progress, TextVariant, status_diamond};
use bolt::tokens::{Borders, DocumentMetrics, Feedback, HeaderMetrics, Spacing, TabMetrics};
use gpui::{
    AnyElement, App, Context, Div, FontWeight, Hsla, IntoElement, ParentElement, Role, Stateful,
    Styled, Window, div, prelude::*,
};
use gpui_kit_assets::IconName;

use super::model::{Phase, RowStatus, Tone};
use super::{RunTab, grid, result, setup};
use crate::shell::request::ui;
use crate::shell::rows::part;
use crate::shell::{Shell, region, text_region};

/// The run document, element id `runner`: the setup, or the header over the results grid
/// and the selected step's result.
pub(in crate::shell) fn render(
    shell: &Shell,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let Some(run) = shell.run.as_ref() else {
        return region("runner").into_any_element();
    };
    let document = region("runner")
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col();
    if run.model.phase == Phase::Setup {
        return document
            .child(setup::render(shell, run, window, cx))
            .into_any_element();
    }
    let selected = run
        .model
        .selected
        .and_then(|step| run.model.result(step).map(|result| (step, result)));
    let lower = match selected {
        Some((step, result)) => result::render(shell, run, step, result, cx),
        None => hint(run, cx),
    };
    document
        .child(header(run, cx))
        .children((run.model.phase == Phase::Running).then(|| progress(run).into_any_element()))
        .child(grid::render(run, cx))
        .child(lower)
        .into_any_element()
}

/// The `bolt` color of `tone`.
pub(super) fn tone_color(tone: Tone, cx: &App) -> Hsla {
    let colors = bolt::colors(cx);
    match tone {
        Tone::Normal => colors.foreground,
        Tone::Accent => bolt::brand::brand(cx).accent_tint,
        Tone::Success => colors.success,
        Tone::Warning => colors.warning,
        Tone::Danger => colors.danger,
        Tone::Muted => colors.muted,
    }
}

fn badge_tone(tone: Tone) -> BadgeTone {
    match tone {
        Tone::Normal | Tone::Muted => BadgeTone::Neutral,
        Tone::Accent => BadgeTone::Accent,
        Tone::Success => BadgeTone::Success,
        Tone::Warning => BadgeTone::Warning,
        Tone::Danger => BadgeTone::Danger,
    }
}

/// The header: the title and badge over the environment and policy line, the totals, and
/// Stop while the run goes on, else Setup and Run again.
fn header(run: &RunTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let model = &run.model;
    let colors = bolt::colors(cx);
    let title = model.title();
    let subline = model.subline().join(" \u{b7} ");
    let heading = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(
            ui::role(text_region("runner-title"), TextVariant::Title, cx)
                .aria_label(title.clone())
                .child(title),
        )
        .children(model.badge().map(|(label, tone)| {
            Badge::new(label, badge_tone(tone))
                .id("runner-badge")
                .aria_label(label)
        }));
    let line = div()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .child(status_diamond(
            bolt::brand::brand(cx).accent_tint,
            Feedback::STATUS_DIAMOND,
        ))
        .child(
            ui::role(text_region("runner-subline"), TextVariant::Caption, cx)
                .aria_label(subline.clone())
                .child(subline),
        );
    let totals = div()
        .flex()
        .items_center()
        .gap(Spacing::LG)
        .font_family(bolt::fonts::editor_family(cx))
        .text_size(DocumentMetrics::TABLE_META_FONT)
        .children(
            model
                .totals()
                .into_iter()
                .zip(["requests", "passed", "failed", "time"])
                .map(|((text, tone), slug)| {
                    part(format!("runner-total-{slug}"))
                        .role(Role::Label)
                        .aria_label(text.clone())
                        .text_color(tone_color(tone, cx))
                        .child(text)
                }),
        );
    let actions: Vec<AnyElement> = if run.is_running() {
        vec![
            Button::new("runner-stop", "Stop")
                .icon(ui::icon(IconName::CircleStop))
                .on_click(cx.listener(|this, _event, _window, cx| this.stop_run(cx)))
                .into_any_element(),
        ]
    } else {
        vec![
            Button::new("runner-back-to-setup", "Setup")
                .on_click(cx.listener(|this, _event, _window, cx| this.show_run_setup(cx)))
                .into_any_element(),
            Button::new("runner-run-again", "Run again")
                .primary()
                .icon(ui::icon(IconName::Play))
                .on_click(cx.listener(|this, _event, _window, cx| this.start_run(cx)))
                .into_any_element(),
        ]
    };
    region("runner-header")
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::XL)
        .pt(HeaderMetrics::SECTION_PADDING_TOP)
        .pb(Spacing::LG)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .border_b(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(HeaderMetrics::SECTION_GAP)
                .child(heading)
                .child(line),
        )
        .child(totals)
        .children(actions)
}

/// How far the run is, `runner-progress`: `bolt`'s bar for the finished requests of the
/// planned ones, under the run header (runner/01).
fn progress(run: &RunTab) -> Div {
    let total = run.model.plan.len().max(1);
    let done = run.model.results.len().min(total);
    #[expect(clippy::cast_precision_loss, reason = "a fraction of a short bar")]
    let fraction = done as f32 / total as f32;
    div()
        .flex_none()
        .px(HeaderMetrics::SECTION_PADDING_X)
        .pb(Spacing::MD)
        .child(
            Progress::new("runner-progress", fraction).label(format!("{done} of {total} requests")),
        )
}

/// The line under the grid while no step is selected, `runner-hint`.
fn hint(run: &RunTab, cx: &App) -> AnyElement {
    let any_done = run
        .model
        .rows()
        .iter()
        .any(|row| matches!(row.status, RowStatus::Done(_)));
    let text = if any_done {
        "Select a finished request to see its tests and response"
    } else {
        "The requests show here as they finish"
    };
    let colors = bolt::colors(cx);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(
            ui::role(text_region("runner-hint"), TextVariant::Caption, cx)
                .aria_label(text)
                .child(text),
        )
        .child(div().flex_1())
        .child(ui::role(div(), TextVariant::Caption, cx).child("each send saved to history"))
        .into_any_element()
}

/// The run's tab chip, `runner-tab`: the play icon, or the spinner while it runs, the target
/// name, `run`, and the close button `runner-tab-close`.
pub(in crate::shell) fn tab_chip(run: &RunTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let close = part("runner-tab-close".to_owned())
        .role(Role::Button)
        .aria_label(format!("Close the run of {}", run.model.name))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(
            Icon::new(ui::icon(IconName::CircleX))
                .size(TabMetrics::CLOSE_ICON)
                .color(colors.muted),
        )
        .on_click(cx.listener(|this, _event, _window, cx| {
            cx.stop_propagation();
            this.close_run_tab(cx);
        }));
    let icon = if run.is_running() {
        IconName::Loader
    } else {
        IconName::Play
    };
    let title = document_tab_title(run.model.name.clone(), run.active, cx);
    let title = if run.active {
        title
    } else {
        title.font_weight(FontWeight::NORMAL)
    };
    document_tab("runner-tab", run.active, cx)
        .role(Role::Tab)
        .aria_label(run.model.tab_label())
        .aria_selected(run.active)
        .font_family(bolt::fonts::ui_family(cx))
        .child(
            Icon::new(ui::icon(icon))
                .size(TabMetrics::ICON)
                .color(bolt::brand::brand(cx).accent_tint),
        )
        .child(title)
        .child(div().flex_none().text_color(colors.muted).child("run"))
        .child(close)
        .on_click(cx.listener(|this, _event, _window, cx| this.activate_run_tab(cx)))
}

/// The collection settings' `Run…` button, `runner-settings-run`, which opens the setup of
/// the collection `id`.
pub(in crate::shell) fn settings_run_button(id: String, cx: &mut Context<Shell>) -> AnyElement {
    Button::new("runner-settings-run", "Run\u{2026}")
        .primary()
        .icon(ui::icon(IconName::Play))
        .on_click(cx.listener(move |this, _event, _window, cx| this.open_run(&id, cx)))
        .into_any_element()
}

impl Shell {
    /// Setup: back to the setup form of the open run document, keeping its setup. Does
    /// nothing while a run is in flight.
    pub(in crate::shell) fn show_run_setup(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut()
            && !run.is_running()
        {
            run.model.phase = Phase::Setup;
            cx.notify();
        }
    }
}
