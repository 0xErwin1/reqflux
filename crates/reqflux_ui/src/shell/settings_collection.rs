//! The Environments block of a collection's settings section (v1-features S7,
//! design/layouts/01 D7): a multi-select over the workspace's environments, empty meaning
//! any, written through the engine as the collection's `environments` list, and the
//! section it renders in. The control reads All environments while the list is empty and
//! Selected only once it names environments; ticking writes the whole list, so new
//! environments stay out until they are ticked.

use bolt::controls::Checkbox;
use bolt::primitives::{Icon, SegmentedItem, TextVariant};
use bolt::tokens::Spacing;
use gpui::{Context, ParentElement, SharedString, Styled, div, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_core::model::Id;

use super::request::segmented;
use super::request::ui;
use super::variables_model::environments;
use crate::shell::{Shell, region};
use crate::workspace::OpenedWorkspace;

/// The group id of the All environments / Selected only control.
const CONTROL: &str = "collection-allowlist";

/// The Environments block of the collection `id`: its allowlist control and one checkbox
/// per workspace environment. Nothing without a loaded workspace.
pub(super) fn allowlist(shell: &Shell, id: Id, cx: &mut Context<Shell>) -> gpui::AnyElement {
    let OpenedWorkspace::Loaded(engine) = &shell.workspace else {
        return div().into_any_element();
    };
    let workspace = engine.workspace();
    let Some(node) = workspace
        .collections
        .iter()
        .find(|node| node.collection.id == id)
    else {
        return div().into_any_element();
    };
    let allowed = &node.collection.environments;
    let name = node.collection.name.clone();
    let colors = bolt::colors(cx);
    let active = if allowed.is_empty() {
        "all"
    } else {
        "selected"
    };
    let items = [
        ("all", "All environments", "collection-allowlist-all"),
        ("selected", "Selected only", "collection-allowlist-selected"),
    ]
    .into_iter()
    .map(|(kind, name, element)| SegmentedItem::new(kind, name).element_id(element.to_owned()))
    .collect();
    let control = segmented::control(
        CONTROL,
        Some("Environment allowlist".into()),
        false,
        items,
        active,
        cx.listener(move |this, choice: &SharedString, _window, cx| {
            // All environments is the empty list; picking Selected only with nothing
            // selected would have nothing to write, so it waits for a tick.
            if choice == "all" {
                this.set_collection_allowlist(id, Vec::new(), cx);
            }
        }),
    );
    let mut chips = div().flex().flex_wrap().gap(Spacing::SM);
    for entry in environments(workspace) {
        let ticked = allowed.contains(&entry.id);
        let shell = cx.entity().downgrade();
        let (collection, environment) = (id, entry.id);
        chips = chips.child(
            Checkbox::new(format!("collection-allowlist-environment-{}", entry.id))
                .checked(ticked)
                .label(entry.name.clone())
                .on_click(move |checked, _window, cx| {
                    if let Err(error) = shell.update(cx, |this, cx| {
                        this.toggle_collection_allowlist_member(
                            collection,
                            environment,
                            *checked,
                            cx,
                        )
                    }) {
                        log::debug!("the allowlist toggle ran after the shell closed: {error}");
                    }
                }),
        );
    }
    region("collection-allowlist-block")
        .flex_none()
        .flex()
        .flex_col()
        .gap(Spacing::MD)
        .pl(Spacing::MD)
        .pr(Spacing::MD)
        .pt(Spacing::SM)
        .pb(Spacing::MD)
        .border_b(bolt::tokens::Borders::THIN)
        .border_color(colors.line)
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::SM)
                .child(
                    Icon::new(ui::icon(IconName::Layers))
                        .size(bolt::tokens::TreeMetrics::ICON)
                        .color(bolt::brand::brand(cx).accent_tint),
                )
                .child(
                    ui::role(
                        region("collection-allowlist-caption"),
                        TextVariant::Label,
                        cx,
                    )
                    .child("ENVIRONMENTS"),
                )
                .child(
                    ui::role(
                        region("collection-allowlist-hint"),
                        TextVariant::Caption,
                        cx,
                    )
                    .child(format!("Requests of {name} may run with")),
                ),
        )
        .child(control)
        .child(chips)
        .child(
            ui::role(
                region("collection-allowlist-note"),
                TextVariant::Caption,
                cx,
            )
            .child(
                "Written as environments = [ids] in collection.toml. New environments stay \
                 out until they are ticked.",
            ),
        )
        .into_any_element()
}

impl Shell {
    /// Writes the environment allowlist of the collection `id`, empty meaning every
    /// environment. The page shows why a refused write failed; the open requests are
    /// inspected again either way, as after a variable write.
    pub(super) fn set_collection_allowlist(
        &mut self,
        id: Id,
        environments: Vec<Id>,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.with_engine(|engine| {
            engine
                .set_collection_environments(id, environments)
                .map(|()| None)
                .map_err(|error| error.to_string())
        });
        match outcome {
            Ok(_) => {
                self.show_variables_warning(None, cx);
                self.inspect_open_requests(cx);
            }
            Err(error) => self.show_variables_error(error, cx),
        }
    }

    /// Ticks or unticks `environment` in the allowlist of the collection `id`, keeping the
    /// workspace order of the environments that stay.
    fn toggle_collection_allowlist_member(
        &mut self,
        id: Id,
        environment: Id,
        member: bool,
        cx: &mut Context<Self>,
    ) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let workspace = engine.workspace();
        let Some(node) = workspace
            .collections
            .iter()
            .find(|node| node.collection.id == id)
        else {
            return;
        };
        let allowed: Vec<Id> = workspace
            .environments
            .iter()
            .map(|node| node.environment.id)
            .filter(|kept| {
                if *kept == environment {
                    member
                } else {
                    node.collection.environments.contains(kept)
                }
            })
            .collect();
        self.set_collection_allowlist(id, allowed, cx);
    }
}

#[cfg(test)]
#[path = "settings_collection_tests.rs"]
mod tests;
