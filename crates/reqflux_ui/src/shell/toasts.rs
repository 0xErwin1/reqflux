//! Toasts (app-pending S9, design/toasts/01): short confirmations and the failures of a user
//! action that would otherwise reach only the log, drawn by `bolt`'s toast stack. The shell
//! owns the [`ToastHost`], publishes it as `bolt`'s [`ToastGlobal`] so [`Toast::push`] finds
//! it, and mounts it over the region between the title bar and the status bar.
//!
//! A toast never carries a secret: titles are fixed sentences with counts and names, and an
//! error's details are the error text, which the engine already masks.

use bolt::toast::{Toast, ToastGlobal, ToastHost};
use gpui::{App, AppContext as _, Entity};

/// Creates the window's toast stack and makes it the one [`Toast::push`] reaches.
pub(super) fn host(cx: &mut App) -> Entity<ToastHost> {
    let host = cx.new(|_| ToastHost::new());
    cx.set_global(ToastGlobal { host: host.clone() });
    host
}

/// A confirmation: `title`, with `subtitle` under it. It closes on its own.
pub(super) fn success(title: impl Into<String>, subtitle: impl Into<String>, cx: &mut App) {
    let (title, subtitle) = (title.into(), subtitle.into());
    #[cfg(test)]
    record(Kind::Success, &title, &subtitle, cx);
    Toast::success(title).subtitle(subtitle).push(cx);
}

/// A failure: `title`, with `details` behind its "Show details" toggle. It stays until the
/// user dismisses it.
pub(super) fn failure(title: impl Into<String>, details: impl Into<String>, cx: &mut App) {
    let (title, details) = (title.into(), details.into());
    log::warn!("{title}: {details}");
    #[cfg(test)]
    record(Kind::Error, &title, &details, cx);
    Toast::error(title).details(details).push(cx);
}

/// What a test reads back of the toasts pushed: `bolt` keeps a toast's texts private and the
/// automation tree reports only its controls, so the push records them here.
#[cfg(test)]
#[derive(Default)]
pub(super) struct Pushed(pub(super) Vec<PushedToast>);

#[cfg(test)]
impl gpui::Global for Pushed {}

/// One pushed toast: its kind, title and subtitle or details.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PushedToast {
    pub(super) kind: Kind,
    pub(super) title: String,
    pub(super) text: String,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Success,
    Error,
}

#[cfg(test)]
fn record(kind: Kind, title: &str, text: &str, cx: &mut App) {
    cx.default_global::<Pushed>().0.push(PushedToast {
        kind,
        title: title.to_owned(),
        text: text.to_owned(),
    });
}

/// `count` followed by `noun`, plural unless `count` is one: `1 request`, `12 requests`.
pub(super) fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
#[path = "toasts_tests.rs"]
pub(super) mod tests;
