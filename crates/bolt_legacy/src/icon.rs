//! Line icons from the Lucide catalog that gpui-component bundles.
//!
//! Icons are SVG assets loaded through GPUI's asset source, so a product installs
//! [`assets`] with `Application::with_assets` before opening a window. Without it, icons
//! paint nothing.

use gpui::prelude::*;
use gpui::{App, AssetSource, Hsla, Pixels, Window, svg};

pub use gpui_kit_assets::IconName;

use crate::theme::colors;
use crate::tokens::icon;

/// The asset source that holds every [`IconName`], including the icons gpui-component's
/// own widgets draw. Pass it to `Application::with_assets`.
pub fn assets() -> impl AssetSource {
    gpui_kit_assets::AllAssets
}

/// A Lucide icon, square, sized by an [`icon`] token and colored by a Bolt role.
#[derive(IntoElement, Clone, Copy, Debug)]
pub struct Icon {
    name: IconName,
    size: Pixels,
    color: Option<Hsla>,
}

impl Icon {
    /// The icon `name` at [`icon::MD`] in the muted role.
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: icon::MD,
            color: None,
        }
    }

    /// The side of the icon. Take it from [`icon`].
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// The stroke color. Take it from a Bolt role.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(colors(cx).muted);
        svg()
            .path(self.name.path())
            .flex_none()
            .size(self.size)
            .text_color(color)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AssetSource, Context, TestAppContext, px, size};

    use super::*;
    use crate::tokens::icon;

    /// Every icon the window shell draws.
    const SHELL_ICONS: &[IconName] = &[
        IconName::Box,
        IconName::Folder,
        IconName::RotateCcwClock,
        IconName::Layers,
        IconName::Bot,
        IconName::FingerprintPattern,
        IconName::Settings,
        IconName::Plus,
        IconName::Search,
        IconName::Bell,
        IconName::ChevronDown,
        IconName::ChevronRight,
        IconName::LayoutGrid,
        IconName::Send,
        IconName::Save,
        IconName::RotateCcw,
        IconName::Undo2,
        IconName::Redo2,
        IconName::Columns2,
    ];

    #[test]
    fn the_asset_source_holds_every_shell_icon() {
        let source = assets();
        for name in SHELL_ICONS {
            let loaded = source.load(&name.path());
            assert!(
                matches!(loaded, Ok(Some(ref bytes)) if bytes.starts_with(b"<svg")),
                "{name:?} at {} did not load",
                name.path()
            );
        }
    }

    #[test]
    fn an_icon_defaults_to_the_medium_size() {
        assert_eq!(Icon::new(IconName::Box).size, icon::MD);
        assert_eq!(Icon::new(IconName::Box).size(icon::RAIL).size, icon::RAIL);
    }

    struct Host;

    impl Render for Host {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
                .flex()
                .items_start()
                .child(
                    gpui::div()
                        .debug_selector(|| "medium".into())
                        .child(Icon::new(IconName::Search)),
                )
                .child(
                    gpui::div()
                        .debug_selector(|| "rail".into())
                        .child(Icon::new(IconName::Bot).size(icon::RAIL)),
                )
        }
    }

    #[gpui::test]
    fn icons_lay_out_at_their_token_size(cx: &mut TestAppContext) {
        let (_host, cx) = cx.add_window_view(|_, _| Host);
        cx.run_until_parked();

        let Some(medium) = cx.debug_bounds("medium") else {
            panic!("the medium icon was not laid out");
        };
        let Some(rail) = cx.debug_bounds("rail") else {
            panic!("the rail icon was not laid out");
        };
        assert_eq!(medium.size, size(px(14.0), px(14.0)));
        assert_eq!(rail.size, size(px(19.0), px(19.0)));
    }
}
