//! The run setup, `runner-setup`: the target, the environment for this run and Stop on
//! first failure, with Cancel and Run; beside it the run order, `runner-order`, grouped by
//! folder. The engine sends the saved requests, so a request with an unsaved draft says
//! its draft is not sent.

use bolt::composites::header::section_header;
use bolt::composites::{MenuItem, field_row_with_desc, menu_frame, menu_row, render_menu_header};
use bolt::controls::{Button, Checkbox, SelectTrigger};
use bolt::primitives::{Icon, TextVariant, status_diamond};
use bolt::tokens::{Borders, DocumentMetrics, Feedback, HeaderMetrics, Spacing, TreeMetrics};
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, ParentElement, Point, Role, Stateful, Styled, Window, anchored, deferred, div,
    prelude::*,
};
use gpui_kit_assets::IconName;

use super::RunTab;
use super::model::{OrderLine, PlanRow};
use crate::shell::request::ui;
use crate::shell::rows::part;
use crate::shell::{Shell, region, text_region};
use crate::tokens::{
    self, RUNNER_FIELD_WIDTH as FIELD_WIDTH, RUNNER_ORDER_WIDTH as ORDER_WIDTH, method_hue,
};
use crate::workspace::OpenedWorkspace;

/// The note under the run order.
const ORDER_NOTE: &str = "Scripts can change the order: pm.setNextRequest(\"name\") jumps, \
                          pm.setNextRequest(null) ends the run.";

/// The setup form beside the run order.
pub(super) fn render(
    shell: &Shell,
    run: &RunTab,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .child(form(shell, run, window, cx))
        .child(order(shell, run, cx))
        .into_any_element()
}

fn form(shell: &Shell, run: &RunTab, window: &Window, cx: &mut Context<Shell>) -> Stateful<Div> {
    let model = &run.model;
    let colors = bolt::colors(cx);
    let count = model.plan.len();
    let title = model.title();
    let description = format!(
        "Sends {count} {} in tree order with their scripts. Variables set by one request \
         carry to the next.",
        if count == 1 { "request" } else { "requests" }
    );
    let target = SelectTrigger::new("runner-target", model.name.clone())
        .leading_icon(ui::icon(
            if model.kind == super::model::TargetKind::Collection {
                IconName::Box
            } else {
                IconName::Folder
            },
        ))
        .aria_label(format!("Run {}, {}", model.name, model.kind.hint()))
        .disabled(true);
    let environment_label = model
        .environment_name
        .clone()
        .unwrap_or_else(|| "No environment".to_owned());
    let environment = div()
        .relative()
        .w(FIELD_WIDTH)
        .child(
            SelectTrigger::new("runner-environment", environment_label.clone())
                .aria_label(format!("Environment: {environment_label}"))
                .focus_ring(
                    run.environment_menu
                        .then(|| bolt::brand::brand(cx).accent_tint),
                )
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.toggle_run_environment_menu(window, cx);
                })),
        )
        .children(
            run.environment_menu
                .then(|| environment_menu(shell, run, window, cx)),
        );
    let bail = Checkbox::new("runner-bail")
        .checked(model.bail)
        .label("Stop on first failure")
        .on_click(cx.listener(|this, checked: &bool, _window, cx| {
            this.set_run_bail(*checked, cx);
        }));
    let fields = div()
        .flex_1()
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .child(section_header(
            "SETUP",
            Some(ui::icon(IconName::SlidersHorizontal)),
            cx,
        ))
        .child(field_row_with_desc(
            "Run",
            div().w(FIELD_WIDTH).child(target),
            model.kind.hint(),
            cx,
        ))
        .child(field_row_with_desc(
            "Environment",
            environment,
            "The window's environment. Changing it here applies to this run only.",
            cx,
        ))
        .child(field_row_with_desc(
            "On failure",
            bail,
            "A failed request or test stops the run; the rest are skipped.",
            cx,
        ));
    let run_label = format!(
        "Run {count} {}",
        if count == 1 { "request" } else { "requests" }
    );
    let footer = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .pl(HeaderMetrics::SECTION_PADDING_X)
        .pr(HeaderMetrics::SECTION_PADDING_X)
        .py(Spacing::LG)
        .border_t(Borders::THIN)
        .border_color(colors.line)
        .child(
            ui::role(text_region("runner-history-note"), TextVariant::Caption, cx)
                .aria_label("Every send is saved to history.")
                .child("Every send is saved to history."),
        )
        .child(div().flex_1())
        .child(
            Button::new("runner-cancel", "Cancel")
                .on_click(cx.listener(|this, _event, _window, cx| this.close_run_tab(cx))),
        )
        .child(
            Button::new("runner-start", run_label)
                .primary()
                .icon(ui::icon(IconName::Play))
                .disabled(count == 0)
                .on_click(cx.listener(|this, _event, _window, cx| this.start_run(cx))),
        );
    region("runner-setup")
        .role(Role::Form)
        .aria_label("Run setup")
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(HeaderMetrics::SECTION_GAP)
                .pt(HeaderMetrics::SECTION_PADDING_TOP)
                .pl(HeaderMetrics::SECTION_PADDING_X)
                .pr(HeaderMetrics::SECTION_PADDING_X)
                .pb(Spacing::MD)
                .child(
                    ui::role(text_region("runner-title"), TextVariant::Title, cx)
                        .aria_label(title.clone())
                        .child(title),
                )
                .child(
                    ui::role(text_region("runner-description"), TextVariant::Body, cx)
                        .aria_label(description.clone())
                        .text_color(colors.muted)
                        .child(description),
                ),
        )
        .child(fields)
        .child(footer)
}

/// The environments of the workspace as `(slug, name)`, sorted by name.
fn environments(shell: &Shell) -> Vec<(String, String)> {
    let mut found: Vec<_> = match &shell.workspace {
        OpenedWorkspace::Loaded(engine) => engine
            .workspace()
            .environments
            .iter()
            .map(|node| (node.slug.clone(), node.environment.name.clone()))
            .collect(),
        OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
    };
    found.sort_by(|left, right| left.1.cmp(&right.1));
    found
}

/// The open environment menu, `runner-environment-menu`, under its field: No environment,
/// `runner-environment-none`, then every environment, `runner-environment-<slug>`.
fn environment_menu(
    shell: &Shell,
    run: &RunTab,
    window: &Window,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let selected = run.model.environment.as_deref();
    let option = |slug: Option<(String, String)>, cx: &mut Context<Shell>| {
        let (id, name) = match &slug {
            Some((slug, name)) => (format!("runner-environment-{slug}"), name.clone()),
            None => (
                "runner-environment-none".to_owned(),
                "No environment".to_owned(),
            ),
        };
        let checked = slug.as_ref().map(|(slug, _)| slug.as_str()) == selected;
        menu_row(
            id,
            &MenuItem::new(name.clone()).checked(checked),
            checked,
            cx,
        )
        .aria_label(name)
        .aria_selected(checked)
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.set_run_environment(slug.clone(), cx);
        }))
    };
    let mut menu = menu_frame(cx)
        .id("runner-environment-menu")
        .debug_selector(|| "runner-environment-menu".to_owned())
        .role(Role::Menu)
        .occlude()
        .w(FIELD_WIDTH)
        .child(render_menu_header(&MenuItem::header("ENVIRONMENTS"), cx))
        .child(option(None, cx))
        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
            if event.keystroke.key == "escape" {
                this.close_run_environment_menu(cx);
            }
        }));
    for environment in environments(shell) {
        menu = menu.child(option(Some(environment), cx));
    }
    let backdrop = region("runner-environment-menu-backdrop")
        .occlude()
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| this.close_run_environment_menu(cx)),
        );
    div()
        .absolute()
        .top_full()
        .left_0()
        .child(deferred(anchored().position(Point::default()).child(backdrop)).with_priority(1))
        .child(deferred(anchored().child(menu)).with_priority(2))
        .into_any_element()
}

/// The run order: a header row per folder with its count, then its requests numbered in
/// run order with their method; requests at the collection root have no header.
fn order(shell: &Shell, run: &RunTab, cx: &mut Context<Shell>) -> Stateful<Div> {
    let colors = bolt::colors(cx);
    let mut rows: Vec<AnyElement> = Vec::new();
    for line in run.model.order() {
        match line {
            OrderLine::Folder { index, name, count } => {
                rows.push(folder_row(index, &name, count, cx));
            }
            OrderLine::Request(index) => {
                if let Some(plan) = run.model.plan.get(index) {
                    let drafted = shell.drafts.contains_key(&plan.id);
                    rows.push(order_row(index + 1, plan, drafted, cx));
                }
            }
        }
    }
    region("runner-order")
        .role(Role::List)
        .aria_label("Run order")
        .flex_none()
        .w(ORDER_WIDTH)
        .flex()
        .flex_col()
        .border_l(Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .pl(Spacing::LG)
                .pr(Spacing::LG)
                .pt(HeaderMetrics::SECTION_PADDING_TOP)
                .pb(Spacing::SM)
                .child(ui::role(div(), TextVariant::Label, cx).child("ORDER"))
                .child(div().flex_1())
                .child(ui::role(div(), TextVariant::Caption, cx).child("tree order")),
        )
        .child(
            div()
                .id("runner-order-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows),
        )
        .child(
            ui::role(text_region("runner-order-note"), TextVariant::Caption, cx)
                .aria_label(ORDER_NOTE)
                .flex_none()
                .p(Spacing::LG)
                .border_t(Borders::THIN)
                .border_color(colors.line)
                .child(ORDER_NOTE),
        )
}

fn folder_row(index: usize, name: &str, count: usize, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    part(format!("runner-order-folder-{index}"))
        .role(Role::Label)
        .aria_label(format!("{name}, {count}"))
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .pl(Spacing::LG)
        .pr(Spacing::LG)
        .text_color(colors.foreground)
        .child(
            Icon::new(ui::icon(IconName::Folder))
                .size(TreeMetrics::ICON)
                .color(colors.muted),
        )
        .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
        .child(
            div()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TreeMetrics::META_FONT)
                .text_color(colors.muted)
                .child(count.to_string()),
        )
        .into_any_element()
}

/// The order row `runner-order-<n>` of the `n`th request.
fn order_row(n: usize, plan: &PlanRow, drafted: bool, cx: &Context<Shell>) -> AnyElement {
    let colors = bolt::colors(cx);
    let label = if drafted {
        format!(
            "{n} {} {}, its unsaved draft is not sent",
            plan.method, plan.name
        )
    } else {
        format!("{n} {} {}", plan.method, plan.name)
    };
    part(format!("runner-order-{n}"))
        .role(Role::ListItem)
        .aria_label(label)
        .flex()
        .items_center()
        .gap(Spacing::SM)
        .h(DocumentMetrics::TABLE_ROW_HEIGHT)
        .pl(Spacing::XL)
        .pr(Spacing::LG)
        .child(
            div()
                .w(Spacing::XL)
                .flex_none()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(TreeMetrics::META_FONT)
                .text_color(colors.muted)
                .child(n.to_string()),
        )
        .child(
            div()
                .flex_none()
                .font_family(bolt::fonts::editor_family(cx))
                .text_size(tokens::METHOD_LABEL_SIZE)
                .font_weight(FontWeight::BOLD)
                .text_color(method_hue(&plan.method, cx))
                .child(plan.method.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(colors.foreground)
                .child(plan.name.clone()),
        )
        .when(drafted, |row| {
            row.child(status_diamond(colors.warning, Feedback::STATUS_DIAMOND))
                .child(ui::role(div(), TextVariant::Caption, cx).child("draft not sent"))
        })
        .into_any_element()
}

impl Shell {
    /// Opens the setup's environment menu, or closes it when open.
    fn toggle_run_environment_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut() {
            run.environment_menu = !run.environment_menu;
            cx.notify();
        }
    }

    fn close_run_environment_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut() {
            run.environment_menu = false;
            cx.notify();
        }
    }

    /// Sets the environment of this run, `None` being No environment, as `(slug, name)`;
    /// the window's active environment is left as it is.
    pub(in crate::shell) fn set_run_environment(
        &mut self,
        environment: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(run) = self.run.as_mut() {
            let (slug, name) = environment.unzip();
            run.model.environment = slug;
            run.model.environment_name = name;
            run.environment_menu = false;
            cx.notify();
        }
    }

    /// Sets Stop on first failure.
    pub(in crate::shell) fn set_run_bail(&mut self, bail: bool, cx: &mut Context<Self>) {
        if let Some(run) = self.run.as_mut() {
            run.model.bail = bail;
            cx.notify();
        }
    }
}
