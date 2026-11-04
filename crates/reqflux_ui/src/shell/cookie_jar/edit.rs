//! Add cookie and edit cookie on Workspace > Cookies (feedback-nov3 S9): the fields of the
//! dialog, the conversion to a [`StoredCookie`] and the save through `Engine::set_cookie`
//! (an add) or `Engine::replace_cookie` (an edit, where a changed domain, path or name moves
//! the cookie). The engine validates; the dialog shows its text under the fields and stays
//! open. The text never holds the cookie value, and nothing here logs one.
//!
//! A value the table hides as a secret opens empty: leaving it empty keeps the stored value,
//! so editing a secret cookie never reveals it.

use bolt::controls::{Dropdown, DropdownItem, InputState};
use gpui::{AppContext as _, Context, Entity, Focusable as _, Window};
use reqflux_engine::StoredCookie;

use super::CookieValue;
use crate::shell::{Shell, toasts};
use crate::workspace::OpenedWorkspace;

/// The `SameSite` choices, as the select shows them and the cookie writes them; the first
/// leaves the attribute out.
const SAME_SITE: [(&str, &str); 4] = [
    ("Not set", ""),
    ("Strict", "Strict"),
    ("Lax", "Lax"),
    ("None", "None"),
];

/// The date format of the expires field, in UTC.
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M";

/// The text a field with a bad date shows.
const DATE_HINT: &str = "Expires must be a UTC date such as 2026-12-31 10:00";

/// The dialog's state while it is open.
pub(crate) struct CookieEdit {
    /// The domain, path and name of the cookie being edited; `None` for Add cookie.
    pub(crate) original: Option<(String, String, String)>,
    pub(crate) name: Entity<InputState>,
    pub(crate) value: Entity<InputState>,
    pub(crate) domain: Entity<InputState>,
    pub(crate) path: Entity<InputState>,
    pub(crate) expires: Entity<InputState>,
    pub(crate) same_site: Entity<Dropdown>,
    pub(crate) session: bool,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    /// Also sent to the subdomains of the domain (no host-only flag).
    pub(crate) subdomains: bool,
    /// The stored value of a cookie the table hides, kept while the value field is empty.
    hidden_value: Option<String>,
    /// What the engine refused, under the fields.
    pub(crate) error: Option<String>,
}

/// The fields of the dialog as text and flags, before they become a cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fields {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) session: bool,
    pub(crate) expires: String,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    pub(crate) subdomains: bool,
    /// `Strict`, `Lax`, `None` or empty for no attribute.
    pub(crate) same_site: String,
}

/// The expires field's text for `expires`: a UTC date, or empty for a session cookie.
pub(crate) fn expires_input(expires: Option<i64>) -> String {
    expires
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|at| at.format(DATE_FORMAT).to_string())
        .unwrap_or_default()
}

/// The seconds since the epoch of the UTC date `text` (`2026-12-31 10:00`, or just the day).
pub(crate) fn parse_expires(text: &str) -> Result<i64, &'static str> {
    let text = text.trim();
    chrono::NaiveDateTime::parse_from_str(text, DATE_FORMAT)
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map(|day| day.and_time(chrono::NaiveTime::MIN))
        })
        .map(|at| at.and_utc().timestamp())
        .map_err(|_| DATE_HINT)
}

/// The cookie the fields describe; the error is the text to show under them.
pub(crate) fn cookie_from(fields: &Fields) -> Result<StoredCookie, &'static str> {
    let expires = if fields.session {
        None
    } else {
        Some(parse_expires(&fields.expires)?)
    };
    Ok(StoredCookie {
        domain: fields.domain.trim().to_owned(),
        host_only: !fields.subdomains,
        path: fields.path.trim().to_owned(),
        name: fields.name.trim().to_owned(),
        value: fields.value.clone(),
        expires,
        secure: fields.secure,
        http_only: fields.http_only,
        same_site: (!fields.same_site.is_empty()).then(|| fields.same_site.clone()),
    })
}

impl CookieEdit {
    fn fields(&self, cx: &gpui::App) -> Fields {
        let text = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let value = text(&self.value);
        Fields {
            name: text(&self.name),
            value: match (&self.hidden_value, value.is_empty()) {
                (Some(kept), true) => kept.clone(),
                _ => value,
            },
            domain: text(&self.domain),
            path: text(&self.path),
            session: self.session,
            expires: text(&self.expires),
            secure: self.secure,
            http_only: self.http_only,
            subdomains: self.subdomains,
            same_site: self
                .same_site
                .read(cx)
                .selected_value()
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }

    /// Whether the value field is empty over a hidden value, for its placeholder.
    pub(crate) fn keeps_hidden_value(&self) -> bool {
        self.hidden_value.is_some()
    }
}

impl Shell {
    /// The `Add cookie` button: opens the dialog over a session cookie for `/`.
    pub(crate) fn open_cookie_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let blank = StoredCookie {
            domain: String::new(),
            host_only: true,
            path: "/".to_owned(),
            name: String::new(),
            value: String::new(),
            expires: None,
            secure: false,
            http_only: false,
            same_site: None,
        };
        self.open_cookie_dialog(None, &blank, None, window, cx);
    }

    /// A row's pencil: opens the dialog over the cookie stored under `domain`, `path` and
    /// `name`, looked up now so a filtered list or a jar that changed since the row was drawn
    /// never opens another cookie. A cookie that is gone opens nothing.
    pub(crate) fn open_cookie_edit(
        &mut self,
        (domain, path, name): (&str, &str, &str),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(cookie) = self.jar_cookies().into_iter().find(|cookie| {
            (
                cookie.domain.as_str(),
                cookie.path.as_str(),
                cookie.name.as_str(),
            ) == (domain, path, name)
        }) else {
            return;
        };
        let hidden = match &self.settings.cookies.masker {
            Some((masker, lacking)) => {
                let shown = super::cookie_value(&cookie.value, |text| masker.mask(text));
                *lacking || shown == CookieValue::Secret
            }
            None => true,
        };
        let original = (
            cookie.domain.clone(),
            cookie.path.clone(),
            cookie.name.clone(),
        );
        let mut seed = cookie.clone();
        let hidden_value = hidden.then(|| std::mem::take(&mut seed.value));
        self.open_cookie_dialog(Some(original), &seed, hidden_value, window, cx);
    }

    fn open_cookie_dialog(
        &mut self,
        original: Option<(String, String, String)>,
        seed: &StoredCookie,
        hidden_value: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field =
            |text: &str, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
                input.update(cx, |state, cx| state.set_value(text, window, cx));
                input
            };
        let items = SAME_SITE
            .iter()
            .map(|(label, value)| DropdownItem::with_value(*label, *value))
            .collect();
        let selected = SAME_SITE
            .iter()
            .position(|(_, value)| *value == seed.same_site.as_deref().unwrap_or_default());
        let same_site = cx.new(|_| {
            Dropdown::new("cookie-edit-same-site")
                .items(items)
                .selected_index(selected.or(Some(0)))
        });
        let name = field(&seed.name, "session", window, cx);
        let focus = name.read(cx).focus_handle(cx);
        let edit = CookieEdit {
            original,
            value: field(&seed.value, "", window, cx),
            domain: field(&seed.domain, "example.com", window, cx),
            path: field(&seed.path, "/", window, cx),
            expires: field(&expires_input(seed.expires), "2026-12-31 10:00", window, cx),
            name,
            same_site,
            session: seed.expires.is_none(),
            secure: seed.secure,
            http_only: seed.http_only,
            subdomains: !seed.host_only,
            hidden_value,
            error: None,
        };
        self.settings.cookies.edit = Some(edit);
        self.settings
            .cookies
            .edit_focus
            .focus(Some(&focus), window, cx);
        cx.notify();
    }

    /// Sets a checkbox of the dialog.
    pub(crate) fn set_cookie_flag(&mut self, flag: CookieFlag, on: bool, cx: &mut Context<Self>) {
        if let Some(edit) = self.settings.cookies.edit.as_mut() {
            match flag {
                CookieFlag::Session => edit.session = on,
                CookieFlag::Secure => edit.secure = on,
                CookieFlag::HttpOnly => edit.http_only = on,
                CookieFlag::Subdomains => edit.subdomains = on,
            }
            cx.notify();
        }
    }

    /// Cancel, Escape or the backdrop of the dialog.
    pub(crate) fn cancel_cookie_edit(&mut self, cx: &mut Context<Self>) {
        if self.settings.cookies.edit.take().is_some() {
            self.settings.cookies.edit_focus.restore(cx);
        }
        cx.notify();
    }

    /// Save: writes the cookie; a refusal shows under the fields and keeps the dialog open.
    pub(crate) fn save_cookie_edit(&mut self, cx: &mut Context<Self>) {
        let (Some(edit), OpenedWorkspace::Loaded(engine)) =
            (self.settings.cookies.edit.as_ref(), &self.workspace)
        else {
            return;
        };
        let result = cookie_from(&edit.fields(cx))
            .map_err(str::to_owned)
            .and_then(|cookie| {
                let saved = match &edit.original {
                    Some((domain, path, name)) => {
                        engine.replace_cookie(domain, path, name, &cookie)
                    }
                    None => engine.set_cookie(&cookie),
                };
                saved.map_err(|error| error.to_string())
            });
        match result {
            Ok(saved) => {
                self.settings.cookies.edit = None;
                self.settings.cookies.edit_focus.restore(cx);
                toasts::success(
                    format!("Saved {}", saved.name),
                    format!("for {}", saved.domain),
                    cx,
                );
            }
            Err(message) => {
                if let Some(edit) = self.settings.cookies.edit.as_mut() {
                    edit.error = Some(message);
                }
            }
        }
        cx.notify();
    }
}

/// A checkbox of the dialog.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CookieFlag {
    Session,
    Secure,
    HttpOnly,
    Subdomains,
}
