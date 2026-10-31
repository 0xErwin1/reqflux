//! The UI automation bridge installed in every window.
//!
//! [`install`] attaches a `gpui-mcp` bridge to a window so `reqflux mcp`, running as
//! the same user, can discover it, read its rendered element tree and inject
//! input. See "UI automation" in ARCHITECTURE.md for the trust model.
//!
//! [`guard_input`] suspends a window's bridge while any agent call waits for the user's
//! approval (mcp-approvals S4). The bridge has no hook to refuse its input, and the
//! events it injects reach GPUI the way programmatic ones do, indistinguishable from the
//! user's; stopping its listener is the only way the agent cannot answer for the user.

use std::collections::HashMap;

use gpui::{App, Global, Window, WindowId};
use gpui_mcp::{AppId, BridgeConfig, BridgeHandle};

const APP_ID: &str = "reqflux";
const DISPLAY_NAME: &str = "ReqFlux";

/// The bridge of one window and the configuration it was installed with, to install it
/// again after [`guard_input`] lifts; `None` while suspended.
struct Bridge {
    handle: Option<BridgeHandle>,
    config: BridgeConfig,
}

/// Bridges keyed by the window they serve.
///
/// `BridgeHandle` is not `Send` and stops its listener when dropped, so the
/// handles are owned on the foreground thread for as long as their window exists.
#[derive(Default)]
struct Bridges(HashMap<WindowId, Bridge>);

impl Global for Bridges {}

/// Installs the automation bridge into `window`.
///
/// The bridge lives until the window closes or the application quits, at which
/// point its listener stops and its discovery descriptor is removed. A failure
/// is logged and leaves the window usable without automation.
pub(crate) fn install(window: &mut Window, cx: &mut App) {
    let app_id = match AppId::new(APP_ID) {
        Ok(app_id) => app_id,
        Err(error) => {
            log::error!("UI automation bridge: invalid app id {APP_ID:?}: {error}");
            return;
        }
    };
    install_with(window, cx, BridgeConfig::new(app_id, DISPLAY_NAME));
}

/// Installs the bridge configured by `config` into `window`.
pub(crate) fn install_with(window: &mut Window, cx: &mut App, config: BridgeConfig) {
    let handle = start(window, cx, &config);
    let window_id = window.window_handle().window_id();
    registry(cx).0.insert(window_id, Bridge { handle, config });
}

/// Starts a bridge configured by `config` in `window`; `None`, logged, when it fails.
fn start(window: &mut Window, cx: &App, config: &BridgeConfig) -> Option<BridgeHandle> {
    match BridgeHandle::install(window, cx, config.clone()) {
        Ok(handle) => {
            log::info!(
                "UI automation bridge listening, descriptor {}",
                handle.endpoint_path().display()
            );
            Some(handle)
        }
        Err(error) => {
            log::error!("UI automation bridge failed to start: {error}");
            None
        }
    }
}

/// Suspends the bridge of `window` while `guarded`, so no agent can read or drive the
/// window, and starts it again when not. Its listener stops and its descriptor goes, so a
/// client sees the window as gone until the guard lifts; it comes back under a new
/// descriptor. Does nothing for a window without a bridge.
pub(crate) fn guard_input(window: &mut Window, cx: &mut App, guarded: bool) {
    if !cx.has_global::<Bridges>() {
        return;
    }
    let window_id = window.window_handle().window_id();
    let Some(bridge) = cx.global_mut::<Bridges>().0.get_mut(&window_id) else {
        return;
    };
    if guarded {
        if let Some(handle) = bridge.handle.take() {
            handle.shutdown();
            log::info!("UI automation bridge suspended while an approval is pending");
        }
        return;
    }
    if bridge.handle.is_some() {
        return;
    }
    let config = bridge.config.clone();
    let handle = start(window, cx, &config);
    if let Some(bridge) = cx.global_mut::<Bridges>().0.get_mut(&window_id) {
        bridge.handle = handle;
    }
}

fn registry(cx: &mut App) -> &mut Bridges {
    if !cx.has_global::<Bridges>() {
        cx.on_window_closed(|cx, window_id| {
            if let Some(handle) = cx
                .default_global::<Bridges>()
                .0
                .remove(&window_id)
                .and_then(|bridge| bridge.handle)
            {
                handle.shutdown();
            }
        })
        .detach();

        // The platform can exit without closing windows one by one, so the
        // descriptors would outlive the process without this.
        cx.on_app_quit(|cx| {
            for (_, bridge) in cx.default_global::<Bridges>().0.drain() {
                if let Some(handle) = bridge.handle {
                    handle.shutdown();
                }
            }
            async {}
        })
        .detach();
    }

    cx.default_global::<Bridges>()
}
