//! The GPUI user interface.

mod brand;
mod shell;
mod startup;
mod theme;
mod tokens;
mod ui_automation;
mod workspace;
#[cfg(test)]
mod workspace_tests;

use std::path::PathBuf;

use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_component::Root;
use reqflux_engine::state::AppState;

use crate::shell::Shell;
use crate::workspace::OpenedWorkspace;

/// The asset source the main window needs: the icons Bolt draws. Pass it to
/// `Application::with_assets` before [`open_main_window`]; without it, icons paint nothing.
pub fn assets() -> impl gpui::AssetSource {
    bolt::assets()
}

/// Installs Bolt with ReqFlux's brand in the theme saved in the user's config file and opens the main
/// window on the workspace at `workspace`. Without one it opens the most recent workspace
/// that loads, else a new managed workspace named `Personal`, else the empty state
/// (standalone-workspaces S2).
///
/// The workspace loads synchronously before the window opens; a load failure is shown in
/// the window, not returned.
pub fn open_main_window(workspace: Option<PathBuf>, cx: &mut App) -> anyhow::Result<()> {
    let state = match AppState::open_default() {
        Ok(state) => Some(state),
        Err(error) => {
            log::warn!("cannot open the app state; drafts stay in memory: {error}");
            None
        }
    };
    let config_path = match reqflux_engine::config::default_path() {
        Ok(path) => Some(path),
        Err(error) => {
            log::warn!("{error}; the theme is not saved");
            None
        }
    };
    let startup_theme = theme::startup(config_path.as_deref(), state.as_ref());
    theme::install(startup_theme.mode, cx);
    shell::bind_keys(cx);

    let bounds = Bounds::centered(None, tokens::MAIN_WINDOW_SIZE, cx);
    let options = WindowOptions {
        app_id: Some("reqflux".into()),
        titlebar: Some(TitlebarOptions {
            title: Some(bolt::brand(cx).name.clone()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
    };

    let recents: Vec<PathBuf> = match state.as_ref().map(AppState::recent_workspaces) {
        Some(Ok(recents)) => recents.into_iter().map(|recent| recent.path).collect(),
        Some(Err(error)) => {
            log::warn!("cannot read the recent workspaces: {error}");
            Vec::new()
        }
        None => Vec::new(),
    };
    let workspace = startup::startup_workspace(
        workspace,
        &recents,
        |path| OpenedWorkspace::open(Some(path)),
        || match reqflux_engine::create_managed_workspace(startup::PERSONAL) {
            Ok((_, dir)) => Some(dir),
            Err(error) => {
                log::warn!("cannot create the {} workspace: {error}", startup::PERSONAL);
                None
            }
        },
    );
    cx.open_window(options, |window, cx| {
        ui_automation::install(window, cx);
        theme::follow_appearance(window, cx);
        let shell = cx.new(|cx| {
            let mut shell = Shell::with_state(workspace, state, cx);
            shell.set_config(config_path, startup_theme.warning);
            shell
        });
        shell.update(cx, |shell, cx| {
            shell.listen_for_approvals(reqflux_engine::approvals::serve, window, cx);
        });
        let focus = shell.read(cx).initial_focus();
        focus.focus(window, cx);
        cx.new(|cx| Root::new(shell, window, cx))
    })?;

    Ok(())
}
