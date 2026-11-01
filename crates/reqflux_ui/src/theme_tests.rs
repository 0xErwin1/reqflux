//! Installing the shared `bolt` beside `bolt_legacy` (shared-bolt S1, S4): the shared theme
//! carries ReqFlux's brand and the interface font size, its rem sizes match DBFlux's, and the
//! gpui-component theme today's screens draw with stays the one `bolt_legacy` installs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bolt::modals::{Modal, ModalSeams};
use bolt::palette::PaletteMode as SharedMode;
use bolt::tokens::{FontSizes, Heights};
use bolt_legacy::PaletteMode;
use gpui::{
    App, Context, FocusHandle, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, WindowAppearance, div, px,
};
use gpui_component::theme::Theme;

use super::{MODAL_ESCAPE_LABEL, install, set_mode};
use crate::tokens::tests::rgb_of;

/// The gpui-component theme as widgets draw it. The stored light and dark configs are left out:
/// the shared install also keeps the rem size there, so a later reload keeps it.
fn drawn_theme(cx: &App) -> serde_json::Value {
    let mut value = match serde_json::to_value(Theme::global(cx)) {
        Ok(value) => value,
        Err(error) => panic!("serialize the theme: {error}"),
    };
    if let Some(object) = value.as_object_mut() {
        object.remove("light_theme");
        object.remove("dark_theme");
    }
    value
}

#[gpui::test]
fn the_shared_brand_is_reqflux_cobalt(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(PaletteMode::Dark, cx);
        let brand = bolt::brand::brand(cx);
        assert_eq!(brand.name.as_ref(), "ReqFlux");
        assert_eq!(rgb_of(brand.accent), 0x2B45B8);
        assert_eq!(rgb_of(brand.accent_hover), 0x3451C8);
        assert_eq!(rgb_of(brand.accent_pressed), 0x1F3389);
        assert_eq!(rgb_of(brand.accent_tint), 0x9DB1FF);
        assert_eq!(rgb_of(brand.on_accent), 0xFFFFFF);
    });
}

#[gpui::test]
fn shared_sizes_resolve_to_dbflux_pixels_at_the_default_font_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(PaletteMode::Dark, cx);
        assert!(cx.has_global::<bolt::fonts::FontSettingsGlobal>());
        assert_eq!(
            bolt::fonts::current(cx).ui_size,
            bolt::fonts::DEFAULT_UI_FONT_SIZE
        );
        assert_eq!(bolt::density::active(cx), bolt::density::Density::Default);

        // `Root` sets each window's rem size from the theme font size.
        let theme = Theme::global(cx);
        let rem = theme.font_size;
        assert_eq!(rem, px(16.0));
        // Kept in the stored configs too, so a reload keeps the rem size.
        assert_eq!(theme.dark_theme.font_size, Some(16.0));
        assert_eq!(theme.light_theme.font_size, Some(16.0));
        for (size, pixels) in [
            (Heights::ROW, 28.0),
            (Heights::CONTROL, 30.0),
            (Heights::TAB, 36.0),
            (Heights::HEADER, 40.0),
            (FontSizes::BASE, 13.0),
            (FontSizes::LABEL, 11.0),
        ] {
            assert_eq!(size.to_pixels(rem), px(pixels));
            assert_eq!(bolt::fonts::ui_px(cx, size), px(pixels));
        }
    });
}

#[gpui::test]
fn the_shared_palette_follows_the_installed_mode(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(PaletteMode::Light, cx);
        assert_eq!(bolt::palette::mode(cx), SharedMode::Light);

        set_mode(PaletteMode::Dark, WindowAppearance::Light, cx);
        assert_eq!(bolt::palette::mode(cx), SharedMode::Dark);
        assert_eq!(bolt_legacy::mode(cx), PaletteMode::Dark);

        set_mode(PaletteMode::System, WindowAppearance::Light, cx);
        assert_eq!(bolt::palette::mode(cx), SharedMode::Light);
        assert_eq!(bolt_legacy::mode(cx), PaletteMode::System);
    });
}

/// A modal whose Escape and Enter count what they ran.
struct ModalHost {
    focus: FocusHandle,
    closed: Arc<AtomicU32>,
    confirmed: Arc<AtomicU32>,
}

impl Render for ModalHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let closed = self.closed.clone();
        let confirmed = self.confirmed.clone();
        div().size_full().child(
            Modal::new("Seams")
                .body(div())
                .footer(div())
                .focus_handle(&self.focus)
                .on_close(move |_, _| {
                    closed.fetch_add(1, Ordering::SeqCst);
                })
                .on_confirm(move |_, _| {
                    confirmed.fetch_add(1, Ordering::SeqCst);
                }),
        )
    }
}

/// `bolt`'s modals answer ReqFlux's keys (shared-bolt S1): the install sets the modal seams
/// with the `Esc` keycap, and Escape closes and Enter confirms an open modal.
#[gpui::test]
fn modals_answer_escape_and_enter(cx: &mut TestAppContext) {
    cx.update(|cx| install(PaletteMode::Dark, cx));
    let label = cx.update(|cx| {
        cx.try_global::<ModalSeams>()
            .and_then(|seams| (seams.escape_label)(cx))
    });
    assert_eq!(label.as_deref(), Some(MODAL_ESCAPE_LABEL));

    let closed = Arc::new(AtomicU32::new(0));
    let confirmed = Arc::new(AtomicU32::new(0));
    let (view, window) = cx.add_window_view({
        let (closed, confirmed) = (closed.clone(), confirmed.clone());
        move |_, cx| ModalHost {
            focus: cx.focus_handle(),
            closed,
            confirmed,
        }
    });
    window.run_until_parked();
    window.update(|window, cx| view.read(cx).focus.clone().focus(window, cx));
    window.run_until_parked();

    window.simulate_keystrokes("escape");
    assert_eq!(closed.load(Ordering::SeqCst), 1, "Escape closes");
    window.simulate_keystrokes("enter");
    assert_eq!(confirmed.load(Ordering::SeqCst), 1, "Enter confirms");
}

/// Today's screens draw with `bolt_legacy`'s theme: installing the shared `bolt` too changes
/// nothing in it, at startup or after a switch.
#[gpui::test]
fn the_legacy_theme_is_what_gpui_component_draws(
    legacy: &mut TestAppContext,
    both: &mut TestAppContext,
) {
    for mode in [PaletteMode::Dark, PaletteMode::Light] {
        legacy.update(|cx| bolt_legacy::install(crate::brand::reqflux(), mode, cx));
        both.update(|cx| install(mode, cx));
        let expected = legacy.update(|cx| drawn_theme(cx));
        assert_eq!(both.update(|cx| drawn_theme(cx)), expected, "{mode:?}");
    }

    legacy.update(|cx| bolt_legacy::set_mode(PaletteMode::Dark, WindowAppearance::Light, cx));
    both.update(|cx| set_mode(PaletteMode::Dark, WindowAppearance::Light, cx));
    let expected = legacy.update(|cx| drawn_theme(cx));
    assert_eq!(
        both.update(|cx| drawn_theme(cx)),
        expected,
        "after a switch"
    );
}
