//! Workspace > Cookies on the settings page (v1-features S5, design/network/01-settings-sections):
//! the workspace's cookie jar as `Engine::cookies` returns it, with a filter, a delete per row
//! (a toast confirms) and Clear all behind a confirmation. The status bar's `N cookies` chip,
//! the workspace menu's `Cookies…`, the palette and the response Cookies view's
//! `Open cookie jar` open it.
//!
//! `Add cookie` and a pencil per row open a dialog with the cookie's fields ([`edit`]).
//!
//! A cookie shows its value unless it is a secret: a value the workspace's masker changes (it
//! holds the value of a secret variable) or that already holds the mask shows as dots with the
//! SECRET badge, the rule the response Cookies view uses ([`cookie_value`]). When some secret
//! variables could not be read from the keyring the masker lacks them, so every value hides.

use bolt::controls::{InputEvent, InputState};
use bolt::modals::ModalFocus;
use gpui::{App, AppContext as _, Context, Entity, Subscription, Window, actions};
use reqflux_engine::{MASK, Masker, StoredCookie};

use super::Section;
use crate::shell::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

#[path = "cookie_jar/edit.rs"]
pub(crate) mod edit;
#[path = "cookie_jar/edit_view.rs"]
pub(crate) mod edit_view;
#[path = "cookie_jar/view.rs"]
pub(crate) mod view;

actions!(
    cookie_jar,
    [
        /// Opens Workspace > Cookies; the response Cookies view's `Open cookie jar` dispatches
        /// it.
        OpenCookieJar,
    ]
);

/// A cookie value as a table shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CookieValue {
    Shown(String),
    /// A value that holds a secret (by the masker), or one already masked: never shown.
    Secret,
}

/// `value` as a table shows it: hidden whole when `mask` changes it or it already holds
/// [`MASK`], else as it is.
pub(crate) fn cookie_value(value: &str, mask: impl Fn(&str) -> String) -> CookieValue {
    let masked = mask(value);
    if masked != value || value.contains(MASK) {
        CookieValue::Secret
    } else {
        CookieValue::Shown(masked)
    }
}

/// One row of the jar table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JarRow {
    /// The 1-based place of the cookie in the whole jar, kept under a filter.
    pub(crate) n: usize,
    pub(crate) domain: String,
    /// Sent to `domain` only (the HOST badge).
    pub(crate) host_only: bool,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) value: CookieValue,
    /// `30 Nov 2026, 10:05 UTC` or `Session`.
    pub(crate) expires: String,
    /// `Secure`, `HttpOnly` and `SameSite=<value>`, the ones it has, in that order.
    pub(crate) flags: Vec<String>,
}

/// When a stored cookie expires, as the table shows it.
pub(crate) fn expires_text(expires: Option<i64>) -> String {
    match expires {
        None => "Session".to_owned(),
        Some(seconds) => chrono::DateTime::from_timestamp(seconds, 0).map_or_else(
            || seconds.to_string(),
            |at| at.format("%-d %b %Y, %H:%M UTC").to_string(),
        ),
    }
}

/// The rows of `cookies` whose domain or name holds `filter` (case-insensitive), each value
/// shown or hidden by `mask`; `hide_all` hides every value.
pub(crate) fn rows(
    cookies: &[StoredCookie],
    filter: &str,
    mask: impl Fn(&str) -> String,
    hide_all: bool,
) -> Vec<JarRow> {
    let filter = filter.trim().to_lowercase();
    cookies
        .iter()
        .enumerate()
        .filter(|(_, cookie)| {
            filter.is_empty()
                || cookie.domain.to_lowercase().contains(&filter)
                || cookie.name.to_lowercase().contains(&filter)
        })
        .map(|(index, cookie)| {
            let value = if hide_all {
                CookieValue::Secret
            } else {
                cookie_value(&cookie.value, &mask)
            };
            let mut flags = Vec::new();
            if cookie.secure {
                flags.push("Secure".to_owned());
            }
            if cookie.http_only {
                flags.push("HttpOnly".to_owned());
            }
            if let Some(same_site) = &cookie.same_site {
                flags.push(format!("SameSite={same_site}"));
            }
            JarRow {
                n: index + 1,
                domain: cookie.domain.clone(),
                host_only: cookie.host_only,
                path: cookie.path.clone(),
                name: cookie.name.clone(),
                value,
                expires: expires_text(cookie.expires),
                flags,
            }
        })
        .collect()
}

/// `7 cookies · 3 domains` without a filter, `1 of 7` with one.
pub(crate) fn count_text(cookies: &[StoredCookie], shown: usize, filtered: bool) -> String {
    if filtered {
        return format!("{shown} of {}", cookies.len());
    }
    let mut domains: Vec<&str> = cookies
        .iter()
        .map(|cookie| cookie.domain.as_str())
        .collect();
    domains.sort_unstable();
    domains.dedup();
    format!(
        "{} \u{b7} {}",
        toasts::counted(cookies.len(), "cookie"),
        toasts::counted(domains.len(), "domain")
    )
}

/// The section's state: the filter field, the masker the values are shown through and the
/// Clear all confirmation.
pub(crate) struct CookieJarState {
    pub(crate) filter: Option<Entity<InputState>>,
    _filter_change: Option<Subscription>,
    /// The workspace's masker and whether it lacks secrets it could not read; read when the
    /// section opens.
    pub(crate) masker: Option<(Masker, bool)>,
    /// Whether the Clear all confirmation is open.
    pub(crate) confirm: bool,
    pub(crate) focus: ModalFocus,
    /// The Add cookie / edit dialog while open.
    pub(crate) edit: Option<edit::CookieEdit>,
    pub(crate) edit_focus: ModalFocus,
}

impl CookieJarState {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            filter: None,
            _filter_change: None,
            masker: None,
            confirm: false,
            focus: ModalFocus::new(cx),
            edit: None,
            edit_focus: ModalFocus::new(cx),
        }
    }

    /// Drops the filter, the masker and the confirmation, as when the workspace changes.
    pub(crate) fn reset(&mut self) {
        self.filter = None;
        self._filter_change = None;
        self.masker = None;
        self.confirm = false;
        self.edit = None;
    }

    /// The filter text.
    pub(crate) fn filter_text(&self, cx: &App) -> String {
        self.filter
            .as_ref()
            .map(|filter| filter.read(cx).value().to_string())
            .unwrap_or_default()
    }
}

impl Shell {
    /// The cookies of the open workspace's jar; empty without one.
    pub(crate) fn jar_cookies(&self) -> Vec<StoredCookie> {
        match &self.workspace {
            OpenedWorkspace::Loaded(engine) => engine.cookies(),
            OpenedWorkspace::None | OpenedWorkspace::Failed { .. } => Vec::new(),
        }
    }

    /// The rows the section shows now.
    pub(crate) fn jar_rows(&self, cx: &App) -> Vec<JarRow> {
        let filter = self.settings.cookies.filter_text(cx);
        let cookies = self.jar_cookies();
        match &self.settings.cookies.masker {
            Some((masker, lacking)) => rows(&cookies, &filter, |text| masker.mask(text), *lacking),
            // Not read yet: nothing shows in clear.
            None => rows(&cookies, &filter, str::to_owned, true),
        }
    }

    /// Opens Workspace > Cookies; does nothing without a workspace.
    pub(crate) fn open_cookie_jar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        let (masker, problem) = engine.workspace_masker();
        if let Some(problem) = &problem {
            log::warn!("the cookie jar hides every value: {problem}");
        }
        self.settings.cookies.masker = Some((masker, problem.is_some()));
        self.ensure_jar_filter(window, cx);
        self.commit_cell_edit(window, cx);
        self.settings.menu = None;
        self.settings.confirm = None;
        self.settings.rename = None;
        self.open_settings(Section::Cookies, cx);
        self.restore_focus(window, cx);
    }

    /// The palette's `Clear cookie jar…`: opens the section and asks to clear it.
    pub(crate) fn ask_clear_cookie_jar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.settings_section(), Section::Cookies) || !self.settings_open(cx) {
            self.open_cookie_jar(window, cx);
        }
        if self.jar_cookies().is_empty() {
            return;
        }
        self.settings.cookies.confirm = true;
        self.settings.cookies.focus.focus(None, window, cx);
        cx.notify();
    }

    /// Cancel, Escape or the backdrop of the confirmation.
    pub(crate) fn cancel_clear_cookie_jar(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.settings.cookies.confirm) {
            self.settings.cookies.focus.restore(cx);
        }
        cx.notify();
    }

    /// Delete N cookies: clears the jar and reports how many went.
    pub(crate) fn confirm_clear_cookie_jar(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.settings.cookies.confirm) {
            return;
        }
        self.settings.cookies.focus.restore(cx);
        if let OpenedWorkspace::Loaded(engine) = &self.workspace {
            match engine.clear_cookies() {
                Ok(count) => toasts::success(
                    "Cleared the cookie jar",
                    format!("Deleted {}", toasts::counted(count, "cookie")),
                    cx,
                ),
                Err(error) => toasts::failure("Cannot clear the cookie jar", error.to_string(), cx),
            }
        }
        cx.notify();
    }

    /// A row's trash button: deletes that cookie at once and confirms it in a toast.
    pub(crate) fn delete_jar_cookie(
        &mut self,
        domain: &str,
        path: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let OpenedWorkspace::Loaded(engine) = &self.workspace else {
            return;
        };
        match engine.delete_cookie(domain, path, name) {
            Ok(_) => toasts::success(format!("Deleted {name}"), format!("for {domain}"), cx),
            Err(error) => toasts::failure(format!("Cannot delete {name}"), error.to_string(), cx),
        }
        cx.notify();
    }

    /// The filter field, made on first use; typing redraws the table.
    fn ensure_jar_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.cookies.filter.is_some() {
            return;
        }
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter by domain or name"));
        let change = cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        self.settings.cookies.filter = Some(input);
        self.settings.cookies._filter_change = Some(change);
    }
}

#[cfg(test)]
#[path = "cookie_jar_tests.rs"]
mod tests;
