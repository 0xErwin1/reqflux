//! GUI startup.

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use gpui::App;

/// Opens the main window on `workspace`, or on the startup default (the last workspace,
/// else `Personal`; the current directory decides nothing), and runs until it closes.
pub(crate) fn open(workspace: Option<PathBuf>) -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let startup_error: Rc<RefCell<Option<anyhow::Error>>> = Rc::default();

    let error_slot = Rc::clone(&startup_error);
    gpui_platform::application()
        .with_assets(reqflux_ui::assets())
        .run(move |cx: &mut App| {
            crate::signals::quit_on_termination(cx);
            if let Err(error) = reqflux_ui::open_main_window(workspace.clone(), cx) {
                error_slot.replace(Some(error));
                cx.quit();
            }
        });

    match startup_error.take() {
        None => ExitCode::SUCCESS,
        Some(error) => {
            eprintln!("reqflux: failed to open the main window: {error:#}");
            ExitCode::FAILURE
        }
    }
}
