//! `Keycap`, a printed keyboard shortcut.

use gpui::prelude::*;
use gpui::{App, FontWeight, Role, SharedString, Window};

use super::hover::named;
use crate::chamfer::Chamfer;
use crate::theme::colors;
use crate::tokens::{border, cut, family, keycap};

/// A shortcut printed on a keycap: JetBrains Mono at `keycap::FONT` in the muted role, on
/// the raised fill cut `cut::KEYCAP`, with a `line_2` line along the bottom. The key text is
/// printed as given, so `Ctrl Shift P` is one keycap.
#[derive(IntoElement, Clone, Debug)]
pub struct Keycap {
    id: SharedString,
    key: SharedString,
}

impl Keycap {
    /// A keycap with element id `id` printing `key`, which is also its accessible label.
    pub fn new(id: impl Into<SharedString>, key: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            key: key.into(),
        }
    }
}

impl RenderOnce for Keycap {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        named(&self.id, Role::Label, self.key.clone())
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .pl(keycap::PADDING_X)
            .pr(keycap::PADDING_X)
            .py(keycap::PADDING_Y)
            .font_family(family::MONO)
            .text_size(keycap::FONT)
            .line_height(keycap::LINE_HEIGHT)
            .font_weight(FontWeight::MEDIUM)
            .text_color(colors.muted)
            .whitespace_nowrap()
            .child(
                Chamfer::new(cut::KEYCAP)
                    .fill(colors.raised)
                    .bottom_edge(colors.line_2, border::THIN),
            )
            .child(self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keycap_prints_its_key_as_given() {
        assert_eq!(Keycap::new("hint", "Ctrl Shift P").key, "Ctrl Shift P");
    }
}
