//! The window's layout modes (v1-features S7, design/layouts/01): Stacked, the request
//! above its response, and Side by side, the request and its response in columns with
//! `bolt`'s splitter standing vertically between them (`SplitAxis::Horizontal`; see
//! `split`).
//!
//! The mode is `layout` in the user's config file, read once when the window learns the
//! file's path and written by the same format-preserving editor that writes `theme`; a
//! name this build does not know reads as the default. The switchers stand in the tab row
//! (`bolt`'s segmented control, element id `segmented-layout`, with the segments
//! `segmented-layout-stacked` and `segmented-layout-side-by-side`) and at the foot of the
//! action strip (`layout-strip`, element ids `layout-strip-stacked` and
//! `layout-strip-side-by-side`); the `Layout: Stacked` and `Layout: Side by side`
//! commands (see `commands`) do the same from the keyboard. Document mode is a later S7
//! surface and is not offered yet.

use std::path::Path;

use bolt::controls::Button;
use bolt::primitives::{SegmentedControl, SegmentedItem};
use gpui::{App, Context, ElementId, IntoElement, Role, Stateful, Styled, prelude::*};
use gpui_kit_assets::IconName;
use reqflux_engine::config::{self, LayoutMode};

use super::request::ui;
use super::{Shell, region, toasts};

/// The modes the switchers offer, in order, with their icon and label. Document mode is
/// not built yet (v1-features S7) and is left out.
const MODES: [(LayoutMode, IconName, &str); 2] = [
    (LayoutMode::Stacked, IconName::Rows2, "Stacked"),
    (LayoutMode::SideBySide, IconName::Columns2, "Side by side"),
];

impl Shell {
    /// Switches the window to `mode` and saves it as `layout` in the user's config file;
    /// a save that fails shows in an error toast. Without a config path the choice is not
    /// saved.
    pub(super) fn set_layout(&mut self, mode: LayoutMode, cx: &mut Context<Self>) {
        if self.split.mode == mode {
            return;
        }
        self.split.mode = mode;
        if let Some(path) = self.config_path.as_deref()
            && let Err(error) = config::set_layout(path, mode)
        {
            toasts::failure("Cannot save the layout", error.to_string(), cx);
        }
        cx.notify();
    }

    /// Reads the layout mode the config file at `path` starts in, keeping the default for
    /// a file without the key, an unknown name, or a file that cannot be read. Without a
    /// path nothing is read.
    pub(super) fn apply_config_layout(&mut self, path: Option<&Path>) {
        let Some(path) = path else {
            return;
        };
        match config::load(path) {
            Ok(loaded) => self.split.mode = loaded.layout.unwrap_or_default(),
            Err(error) => log::warn!("{error}; using the default layout"),
        }
    }
}

/// The key that switches to `mode`, as the commands registry binds it: ` (Ctrl+Alt+1)`,
/// or empty while the command is not in the registry.
fn key_hint(mode: LayoutMode, cx: &App) -> String {
    let id = match mode {
        LayoutMode::Stacked => super::commands::CommandId::LayoutStacked,
        LayoutMode::SideBySide => super::commands::CommandId::LayoutSideBySide,
    };
    let key = super::commands::effective_key(cx, id);
    if key.is_empty() {
        String::new()
    } else {
        format!(" ({})", super::commands::key_label(&key))
    }
}

/// The tab-row switcher, element id `segmented-layout`: `bolt`'s segmented control with
/// one segment per mode, at the end of the tab row. A click switches the window's layout.
pub(super) fn tab_row_switcher(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let handle = cx.entity();
    let items = MODES
        .map(|(mode, icon, label)| SegmentedItem::new(mode.name(), label).icon(ui::icon(icon)))
        .to_vec();
    SegmentedControl::new(items, shell.split.mode.name(), move |id, _window, cx| {
        let Some(mode) = LayoutMode::from_name(id) else {
            return;
        };
        handle.update(cx, |shell, cx| shell.set_layout(mode, cx));
    })
    .group("layout")
    .aria_label("Layout")
}

/// Adds the layout switcher at the foot of the action `strip` (design/layouts/01): a
/// vertical stack of icon-only buttons pinned to the bottom of the strip.
pub(super) fn with_strip_switcher(
    strip: Stateful<gpui::Div>,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<gpui::Div> {
    let current = shell.split.mode;
    strip.child(
        region("layout-strip")
            .role(Role::RadioGroup)
            .aria_label("Layout")
            .mt_auto()
            .flex()
            .flex_col()
            .items_center()
            .children(MODES.map(|(mode, icon, label)| {
                strip_button(
                    mode,
                    format!("layout-strip-{}", mode.name()),
                    icon,
                    label,
                    current,
                    cx,
                )
            })),
    )
}

/// One strip button of `mode`: an icon-only ghost button, selected while it is the
/// window's mode, sized as the strip's other actions.
fn strip_button(
    mode: LayoutMode,
    id: String,
    icon: IconName,
    label: &'static str,
    current: LayoutMode,
    cx: &mut Context<Shell>,
) -> Button {
    let hint = key_hint(mode, cx);
    Button::new(ElementId::Name(id.into()), label)
        .ghost()
        .icon_only()
        .inline()
        .icon(ui::icon(icon))
        .tooltip(format!("{label}{hint}"))
        .aria_label(label)
        .selected(mode == current)
        .on_click(cx.listener(move |this, _event, _window, cx| this.set_layout(mode, cx)))
}
