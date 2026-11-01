//! Installing `bolt` (shared-bolt S1, S4): the theme carries ReqFlux's brand and the interface
//! font size, its rem sizes match DBFlux's, and the gpui-component theme every widget draws
//! with is DBFlux's palette with ReqFlux's accent.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bolt::Colors;
use bolt::modals::{Modal, ModalSeams};
use bolt::tokens::{FontSizes, Heights};
use gpui::{
    App, Context, FocusHandle, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, WindowAppearance, div, px,
};
use gpui_component::theme::Theme;

use super::{
    MODAL_ESCAPE_LABEL, PaletteMode, font_settings, install, mode, set_font_size, set_mode, startup,
};
use crate::tokens::tests::rgb_of;

/// The gpui-component theme as widgets draw it. The stored light and dark configs are left out:
/// the install also keeps the rem size there, so a later reload keeps it.
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
fn the_brand_is_reqflux_cobalt(cx: &mut TestAppContext) {
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
fn sizes_resolve_to_dbflux_pixels_at_the_default_font_size(cx: &mut TestAppContext) {
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

/// The interface font size of the config file resizes every `bolt` size, as DBFlux's does
/// (app-pending S3).
#[gpui::test]
fn the_interface_font_size_scales_the_rem_live(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(PaletteMode::Dark, cx);
        set_font_size(15.0, cx);
        let fonts = bolt::fonts::current(cx);
        assert_eq!(
            (fonts.ui_size, fonts.editor_size, fonts.grid_size),
            (15.0, 15.0, 14.5)
        );
        assert_eq!(bolt::fonts::ui_px(cx, FontSizes::BASE), px(15.0));
        let rem = Theme::global(cx).font_size;
        assert_eq!(
            FontSizes::BASE.to_pixels(rem),
            px(15.0),
            "the theme's rem follows"
        );
    });
    assert_eq!(
        font_settings(99.0).ui_size,
        32.0,
        "bounded like the config file"
    );
    assert_eq!(font_settings(f32::NAN).ui_size, 13.0);
}

#[test]
fn startup_reads_the_font_size_of_the_config_file() {
    let dir = match tempfile::TempDir::new() {
        Ok(dir) => dir,
        Err(error) => panic!("temp dir: {error}"),
    };
    let path = dir.path().join("config.toml");
    assert_eq!(startup(Some(&path), None).font_size, 13.0, "a missing file");
    if let Err(error) = std::fs::write(&path, "font_size = 16\n") {
        panic!("write: {error}");
    }
    assert_eq!(startup(Some(&path), None).font_size, 16.0);
    if let Err(error) = std::fs::write(&path, "font_size = \"big\"\n") {
        panic!("write: {error}");
    }
    let broken = startup(Some(&path), None);
    assert_eq!(
        broken.font_size, 13.0,
        "a file that does not parse is the default"
    );
    assert!(broken.config.error.is_some());
}

#[gpui::test]
fn the_palette_follows_the_installed_mode(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(PaletteMode::Light, cx);
        assert_eq!(bolt::palette::mode(cx), PaletteMode::Light);
        assert_eq!(bolt::colors(cx), Colors::light());

        set_mode(PaletteMode::Dark, WindowAppearance::Light, cx);
        assert_eq!(bolt::palette::mode(cx), PaletteMode::Dark);
        assert_eq!(mode(cx), PaletteMode::Dark);
        assert_eq!(bolt::colors(cx), Colors::dark());

        // `bolt` keeps the resolved palette; the selection stays System.
        set_mode(PaletteMode::System, WindowAppearance::Light, cx);
        assert_eq!(bolt::palette::mode(cx), PaletteMode::Light);
        assert_eq!(mode(cx), PaletteMode::System);
        assert_eq!(bolt::colors(cx), Colors::light());
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

/// DBFlux's theme with ReqFlux's brand: `bolt`'s install alone, as DBFlux runs it.
fn dbflux_palette(mode: PaletteMode, cx: &mut App) {
    if !cx.has_global::<Theme>() {
        gpui_component::init(cx);
    }
    bolt::theme::install(
        crate::brand::reqflux(),
        mode,
        bolt::density::Density::Default,
        cx,
    );
}

/// What gpui-component draws is DBFlux's palette with ReqFlux's accent, at startup and after
/// a switch: ReqFlux overrides no field of the theme `bolt` installs.
#[gpui::test]
fn the_drawn_theme_is_dbflux_palette_with_the_reqflux_accent(
    dbflux: &mut TestAppContext,
    reqflux: &mut TestAppContext,
) {
    for mode in [PaletteMode::Dark, PaletteMode::Light] {
        dbflux.update(|cx| dbflux_palette(mode, cx));
        reqflux.update(|cx| install(mode, cx));
        let expected = dbflux.update(|cx| drawn_theme(cx));
        assert_eq!(reqflux.update(|cx| drawn_theme(cx)), expected, "{mode:?}");
        reqflux.update(|cx| {
            assert_eq!(rgb_of(Theme::global(cx).primary), 0x2B45B8, "{mode:?}");
        });
    }

    dbflux.update(|cx| dbflux_palette(PaletteMode::Dark, cx));
    reqflux.update(|cx| set_mode(PaletteMode::Dark, WindowAppearance::Light, cx));
    let expected = dbflux.update(|cx| drawn_theme(cx));
    assert_eq!(
        reqflux.update(|cx| drawn_theme(cx)),
        expected,
        "after a switch"
    );
}
