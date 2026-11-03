//! The per-workspace cookie jar (v1-features S5).
//!
//! Every engine has a jar its client consults on every hop of every send (the main request,
//! its auth requests and the scripts' `pm.sendRequest`): the `Set-Cookie` headers a response
//! carries are stored, and the cookies matching a request's URL are sent in its `Cookie`
//! header after the request's own. Matching follows RFC 6265 (domain, path, `Secure`,
//! `HttpOnly`, expiry) through `cookie_store`, the store `reqwest`'s own jar uses. No public
//! suffix list is loaded, so a `Domain` attribute naming a public suffix is not refused.
//!
//! The jar lives in memory until [`Engine::persist_cookies`] attaches the app state: it then
//! loads the workspace's stored cookies and writes every change through, one row per cookie,
//! so the cookies outlive the engine and are shared by the engines of the same workspace that
//! attach it later. Cookies are never in a workspace file. A cookie value is a credential:
//! history records the `Cookie` header masked whole, and [`StoredCookie::masked`] hides the
//! value for output an agent reads.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use cookie_store::{Cookie, CookieStore};
use url::Url;

use crate::engine::Engine;
use crate::sent::{MASK, Masker, SentRequest};
use crate::state::{AppState, CookieRow, StateError};

/// One cookie of the jar.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredCookie {
    /// The domain it is stored under: the host that set it when `host_only`, else its
    /// `Domain` attribute without a leading dot.
    pub domain: String,
    /// Whether it is sent to `domain` only, not to its subdomains (no `Domain` attribute).
    pub host_only: bool,
    /// The cookie path.
    pub path: String,
    /// The cookie name.
    pub name: String,
    /// The cookie value; a credential, see [`StoredCookie::masked`].
    pub value: String,
    /// When it expires, in seconds since the Unix epoch; `None` for a session cookie, which
    /// the jar keeps until it is deleted.
    pub expires: Option<i64>,
    /// `Secure`: sent over HTTPS only.
    pub secure: bool,
    /// `HttpOnly`.
    pub http_only: bool,
    /// `SameSite`, as written (`Strict`, `Lax` or `None`), when set.
    pub same_site: Option<String>,
}

impl std::fmt::Debug for StoredCookie {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoredCookie")
            .field("domain", &self.domain)
            .field("host_only", &self.host_only)
            .field("path", &self.path)
            .field("name", &self.name)
            .field("value", &MASK)
            .field("expires", &self.expires)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("same_site", &self.same_site)
            .finish()
    }
}

impl StoredCookie {
    /// This cookie with its value replaced by [`MASK`], for output an agent reads.
    #[must_use]
    pub fn masked(&self) -> Self {
        Self {
            value: MASK.to_owned(),
            ..self.clone()
        }
    }

    fn of(cookie: &Cookie<'_>) -> Self {
        let expires = match &cookie.expires {
            cookie_store::CookieExpiration::AtUtc(at) => Some(at.unix_timestamp()),
            cookie_store::CookieExpiration::SessionEnd => None,
        };
        Self {
            domain: String::from(&cookie.domain),
            host_only: matches!(cookie.domain, cookie_store::CookieDomain::HostOnly(_)),
            path: String::from(&cookie.path),
            name: cookie.name().to_owned(),
            value: cookie.value().to_owned(),
            expires,
            secure: cookie.secure().unwrap_or(false),
            http_only: cookie.http_only().unwrap_or(false),
            same_site: cookie.same_site().map(|same_site| same_site.to_string()),
        }
    }
}

/// Why a cookie jar operation failed. The text never holds a cookie value.
#[derive(Debug, thiserror::Error)]
pub enum CookieError {
    /// The URL a cookie is added for does not parse or is not HTTP.
    #[error("cannot add a cookie for {0}: it is not an http or https URL")]
    InvalidUrl(String),
    /// The `Set-Cookie` text was refused: it does not parse, it is expired, or its domain
    /// does not match the URL.
    #[error("the cookie was not added: {0}")]
    Rejected(String),
    /// A cookie written from its fields is not valid; the text names the field, never a value.
    #[error("the cookie was not saved: {0}")]
    Invalid(&'static str),
    /// The app state could not be read or written.
    #[error(transparent)]
    State(#[from] StateError),
}

/// The jar of one workspace. Its `Debug` output shows how many cookies it holds, never one.
pub(crate) struct WorkspaceCookies {
    workspace_id: String,
    store: Mutex<CookieStore>,
    state: Mutex<Option<AppState>>,
}

impl std::fmt::Debug for WorkspaceCookies {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.lock().iter_any().count();
        formatter
            .debug_struct("WorkspaceCookies")
            .field("workspace_id", &self.workspace_id)
            .field("cookies", &count)
            .finish_non_exhaustive()
    }
}

impl WorkspaceCookies {
    /// An empty jar for the workspace `workspace_id`, kept in memory.
    pub(crate) fn new(workspace_id: String) -> Self {
        Self {
            workspace_id,
            store: Mutex::new(CookieStore::new()),
            state: Mutex::new(None),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CookieStore> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Option<AppState>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Attaches the app state `state`: its unexpired cookies of this workspace join the jar
    /// (a cookie already in memory with the same domain, path and name wins and is written
    /// back), its expired ones are deleted, and every later change is written through.
    fn attach(&self, mut state: AppState) -> Result<(), StateError> {
        let mut store = self.lock();
        let mut cookies: Vec<Cookie<'static>> = Vec::new();
        for row in state.cookies(&self.workspace_id)? {
            match serde_json::from_str::<Cookie<'static>>(&row.payload) {
                Ok(cookie) if !cookie.is_expired() => cookies.push(cookie),
                Ok(_) => {
                    state.delete_cookie(&self.workspace_id, &row.domain, &row.path, &row.name)?;
                }
                // The decoder's text can quote the payload, which holds the value.
                Err(_) => log::warn!(
                    "the stored cookie {} of {} does not decode and is skipped",
                    row.name,
                    row.domain
                ),
            }
        }
        let in_memory: Vec<Cookie<'static>> = store.iter_unexpired().cloned().collect();
        for cookie in &in_memory {
            let row = row(cookie, state.path())?;
            state.put_cookie(&self.workspace_id, &row)?;
        }
        cookies.extend(in_memory);
        let merged = CookieStore::from_cookies(
            cookies.into_iter().map(Ok::<_, std::convert::Infallible>),
            false,
        );
        *store = match merged {
            Ok(merged) => merged,
            Err(never) => match never {},
        };
        *self.state() = Some(state);
        Ok(())
    }

    /// The unexpired cookies, by domain, path and name.
    fn list(&self) -> Vec<StoredCookie> {
        let mut cookies: Vec<StoredCookie> =
            self.lock().iter_unexpired().map(StoredCookie::of).collect();
        cookies.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        cookies
    }

    /// Parses `set_cookie` as received from `url` and stores it under RFC 6265's rules, writing
    /// the change through to the attached state.
    fn insert(&self, url: &Url, set_cookie: &str) -> Result<Option<StoredCookie>, CookieError> {
        let cookie = Cookie::parse(set_cookie, url)
            .map_err(|error| CookieError::Rejected(error.to_string()))?
            .into_owned();
        self.insert_cookie(url, cookie)
    }

    /// Stores `cookie` as received from `url`, writing the change through to the attached
    /// state.
    fn insert_cookie(
        &self,
        url: &Url,
        cookie: Cookie<'static>,
    ) -> Result<Option<StoredCookie>, CookieError> {
        let (domain, path, name) = (
            String::from(&cookie.domain),
            String::from(&cookie.path),
            cookie.name().to_owned(),
        );
        let mut store = self.lock();
        let action = store
            .insert(cookie, url)
            .map_err(|error| CookieError::Rejected(error.to_string()))?;
        let mut state = self.state();
        if let cookie_store::StoreAction::ExpiredExisting = action {
            store.remove(&domain, &path, &name);
            if let Some(state) = state.as_mut() {
                state.delete_cookie(&self.workspace_id, &domain, &path, &name)?;
            }
            return Ok(None);
        }
        let Some(stored) = store.get_any(&domain, &path, &name) else {
            return Ok(None);
        };
        if let Some(state) = state.as_mut() {
            let row = row(stored, state.path())?;
            state.put_cookie(&self.workspace_id, &row)?;
        }
        Ok(Some(StoredCookie::of(stored)))
    }

    fn delete(&self, domain: &str, path: &str, name: &str) -> Result<bool, StateError> {
        let mut store = self.lock();
        let removed = store.remove(domain, path, name).is_some();
        let stored = match self.state().as_mut() {
            Some(state) => state.delete_cookie(&self.workspace_id, domain, path, name)?,
            None => false,
        };
        Ok(removed || stored)
    }

    fn clear(&self) -> Result<usize, StateError> {
        let mut store = self.lock();
        let count = store.iter_unexpired().count();
        store.clear();
        let stored = match self.state().as_mut() {
            Some(state) => state.clear_cookies(&self.workspace_id)?,
            None => 0,
        };
        Ok(count.max(stored))
    }

    /// The `Cookie` header value the jar sends to `url`, if any cookie matches.
    fn header(&self, url: &Url) -> Option<String> {
        let store = self.lock();
        let pairs: Vec<String> = store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }
}

impl reqflux_http::CookieJar for WorkspaceCookies {
    fn cookie_header(&self, url: &Url) -> Option<String> {
        self.header(url)
    }

    fn store(&self, url: &Url, set_cookies: &[String]) {
        let host = url.host_str().unwrap_or_default();
        for set_cookie in set_cookies {
            // Neither error carries a cookie value: the jar's are fixed texts and the
            // state's name the file.
            match self.insert(url, set_cookie) {
                Ok(_) => {}
                Err(CookieError::State(error)) => {
                    log::warn!("a cookie from {host} was kept in memory only: {error}");
                }
                Err(error) => log::debug!("a cookie from {host} was not stored: {error}"),
            }
        }
    }
}

/// The cookie jar entry for the fields of `cookie`, with the URL it is stored as received from.
fn build(cookie: &StoredCookie) -> Result<(Url, Cookie<'static>), CookieError> {
    let name = cookie.name.trim();
    if name.is_empty() {
        return Err(CookieError::Invalid("the name is empty"));
    }
    let domain = cookie.domain.trim().trim_start_matches('.');
    if domain.is_empty() {
        return Err(CookieError::Invalid("the domain is empty"));
    }
    if !cookie.path.starts_with('/') {
        return Err(CookieError::Invalid("the path must start with /"));
    }
    let separator = |c: char| c.is_control() || c.is_whitespace() || matches!(c, ';' | '=' | ',');
    if name.contains(separator) {
        return Err(CookieError::Invalid(
            "the name has a separator or control character",
        ));
    }
    if cookie.value.contains(|c: char| c.is_control() || c == ';') {
        return Err(CookieError::Invalid(
            "the value has a separator or control character",
        ));
    }
    let url = Url::parse(&format!("https://{domain}/"))
        .ok()
        .filter(|url| {
            url.host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case(domain))
        })
        .ok_or(CookieError::Invalid("the domain is not a valid host"))?;
    // `cookie_store` does not re-export `SameSite`, so the value comes from parsing its text.
    let same_site = match cookie.same_site.as_deref() {
        None | Some("") => None,
        Some(text) => cookie_store::RawCookie::parse(format!("x=y; SameSite={text}"))
            .ok()
            .and_then(|parsed| parsed.same_site())
            .ok_or(CookieError::Invalid("SameSite is not Strict, Lax or None"))
            .map(Some)?,
    };
    let mut raw = cookie_store::RawCookie::build((name.to_owned(), cookie.value.clone()))
        .path(cookie.path.clone())
        .secure(cookie.secure)
        .http_only(cookie.http_only);
    if !cookie.host_only {
        raw = raw.domain(domain.to_owned());
    }
    if let Some(same_site) = same_site {
        raw = raw.same_site(same_site);
    }
    if let Some(expires) = cookie.expires {
        let at = time::OffsetDateTime::from_unix_timestamp(expires)
            .map_err(|_| CookieError::Invalid("the expiry date is out of range"))?;
        if at <= time::OffsetDateTime::now_utc() {
            return Err(CookieError::Invalid("it has already expired"));
        }
        raw = raw.expires(at);
    }
    let built = Cookie::try_from_raw_cookie(&raw.build(), &url)
        .map_err(|error| CookieError::Rejected(error.to_string()))?;
    Ok((url, built.into_owned()))
}

/// The row of `cookie` in the state file `path`.
fn row(cookie: &Cookie<'_>, path: &Path) -> Result<CookieRow, StateError> {
    let payload = serde_json::to_string(cookie).map_err(|_| StateError::Corrupt {
        path: path.to_owned(),
        reason: "a cookie could not be encoded".to_owned(),
    })?;
    Ok(CookieRow {
        domain: String::from(&cookie.domain),
        path: String::from(&cookie.path),
        name: cookie.name().to_owned(),
        payload,
    })
}

/// Adds the `Cookie` header the jar sends to `url` to the recorded request `sent`, masked
/// whole as every `Cookie` header is, unless the request already records one.
pub(crate) fn record_jar_cookie(
    jar: &WorkspaceCookies,
    url: &str,
    sent: &mut SentRequest,
    masker: &Masker,
) {
    let recorded = sent
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("cookie"));
    if recorded {
        return;
    }
    if let Some(value) = Url::parse(url).ok().and_then(|url| jar.header(&url)) {
        sent.headers.push(masker.header("Cookie", &value));
    }
}

impl Engine {
    /// Keeps this workspace's cookie jar in the app state at `state`: the cookies stored
    /// there join the jar, and every change from now on is written through. Without this
    /// call the jar lives as long as the engine.
    ///
    /// # Errors
    ///
    /// The state could not be opened, read or written; the jar stays in memory.
    pub fn persist_cookies(&self, state: &Path) -> Result<(), CookieError> {
        let state = AppState::open(state)?;
        Ok(self.cookies.attach(state)?)
    }

    /// The cookies of the jar, by domain, path and name; expired cookies are left out.
    #[must_use]
    pub fn cookies(&self) -> Vec<StoredCookie> {
        self.cookies.list()
    }

    /// Adds a cookie to the jar as if a response from `url` had sent the `Set-Cookie` value
    /// `set_cookie`, following the same rules. Returns the cookie stored, or `None` when the
    /// value expired (and so deleted) the cookie it names.
    ///
    /// # Errors
    ///
    /// The URL is not HTTP, the value is refused, or the app state failed.
    pub fn add_cookie(
        &self,
        url: &str,
        set_cookie: &str,
    ) -> Result<Option<StoredCookie>, CookieError> {
        let parsed = Url::parse(url)
            .ok()
            .filter(|parsed| matches!(parsed.scheme(), "http" | "https"))
            .ok_or_else(|| CookieError::InvalidUrl(url.to_owned()))?;
        self.cookies.insert(&parsed, set_cookie)
    }

    /// Writes `cookie` to the jar from its fields, replacing the one with its domain, path
    /// and name, and returns the cookie stored. This is the Cookies page's "Add cookie".
    ///
    /// # Errors
    ///
    /// A field is invalid (empty name or domain, a path that does not start with `/`, a name
    /// or value with a separator or control character, an unknown `SameSite`, a date already
    /// past), or the app state failed. The text never holds the value.
    pub fn set_cookie(&self, cookie: &StoredCookie) -> Result<StoredCookie, CookieError> {
        let (url, built) = build(cookie)?;
        self.cookies
            .insert_cookie(&url, built)?
            .ok_or(CookieError::Invalid("it has already expired"))
    }

    /// Replaces the cookie `name` stored under `domain` and `path` with `cookie`; a changed
    /// domain, path or name moves it. The new cookie is validated and stored first, so a
    /// refusal keeps the original.
    ///
    /// # Errors
    ///
    /// The same as [`Engine::set_cookie`].
    pub fn replace_cookie(
        &self,
        domain: &str,
        path: &str,
        name: &str,
        cookie: &StoredCookie,
    ) -> Result<StoredCookie, CookieError> {
        let stored = self.set_cookie(cookie)?;
        if (domain, path, name)
            != (
                stored.domain.as_str(),
                stored.path.as_str(),
                stored.name.as_str(),
            )
        {
            self.cookies.delete(domain, path, name)?;
        }
        Ok(stored)
    }

    /// Deletes the cookie `name` stored under `domain` and `path`, reporting whether there
    /// was one.
    ///
    /// # Errors
    ///
    /// The app state failed.
    pub fn delete_cookie(&self, domain: &str, path: &str, name: &str) -> Result<bool, CookieError> {
        Ok(self.cookies.delete(domain, path, name)?)
    }

    /// Deletes every cookie of the jar, returning how many there were.
    ///
    /// # Errors
    ///
    /// The app state failed.
    pub fn clear_cookies(&self) -> Result<usize, CookieError> {
        Ok(self.cookies.clear()?)
    }
}

/// The jar an engine for `workspace_id` starts with, installed in `client`.
pub(crate) fn install(
    client: &reqflux_http::HttpClient,
    workspace_id: String,
) -> Arc<WorkspaceCookies> {
    let jar = Arc::new(WorkspaceCookies::new(workspace_id));
    let hook: Arc<dyn reqflux_http::CookieJar> = jar.clone();
    client.set_cookie_jar(Some(hook));
    jar
}
