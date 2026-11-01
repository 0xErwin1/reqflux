//! The `Set-Cookie` headers of a response parsed for the Cookies sub-tab, without GPUI, and
//! the table's columns with a secret value masked. The parse is read-only: no cookie is
//! stored (the cookie jar is v1).

use reqflux_engine::MASK;

/// One `Set-Cookie` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Cookie {
    Parsed(SetCookie),
    /// A header that does not parse, as received.
    Raw(String),
}

/// A parsed `Set-Cookie` header; attribute values are as received.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SetCookie {
    pub(crate) name: String,
    /// The value, without surrounding double quotes.
    pub(crate) value: String,
    pub(crate) domain: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) expires: Option<String>,
    pub(crate) max_age: Option<String>,
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    pub(crate) same_site: Option<String>,
}

/// A cookie value as the table shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CookieValue {
    Shown(String),
    /// A value that holds a secret the send knows, or one history stored masked: never
    /// shown.
    Secret,
}

/// The columns of one row of the cookie table (response-views/02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CookieColumns {
    pub(crate) name: String,
    pub(crate) value: CookieValue,
    pub(crate) domain: String,
    pub(crate) path: String,
    pub(crate) expires: String,
    /// `HttpOnly`, `Secure` and `SameSite=<value>`, in that order, the ones it has.
    pub(crate) flags: Vec<String>,
}

/// When a cookie expires as the table shows it: `Max-Age <seconds>` when it has one (it
/// wins over `Expires`), else the `Expires` date as `21 Oct 2026, 07:28 UTC` (as received
/// when it does not parse), else `Session`.
pub(crate) fn expires_text(expires: Option<&str>, max_age: Option<&str>) -> String {
    match (max_age, expires) {
        (Some(max_age), _) => format!("Max-Age {max_age}"),
        (None, Some(expires)) => chrono::DateTime::parse_from_rfc2822(expires).map_or_else(
            |_| expires.to_owned(),
            |date| {
                date.with_timezone(&chrono::Utc)
                    .format("%-d %b %Y, %H:%M UTC")
                    .to_string()
            },
        ),
        (None, None) => "Session".to_owned(),
    }
}

impl SetCookie {
    /// The table columns, every text masked by `mask` (the send's `Masker::mask`) and the
    /// value hidden whole when masking changes it or it already holds [`MASK`].
    pub(crate) fn columns(&self, mask: impl Fn(&str) -> String) -> CookieColumns {
        let masked = mask(&self.value);
        let value = if masked != self.value || self.value.contains(MASK) {
            CookieValue::Secret
        } else {
            CookieValue::Shown(masked)
        };
        let mut flags = Vec::new();
        if self.http_only {
            flags.push("HttpOnly".to_owned());
        }
        if self.secure {
            flags.push("Secure".to_owned());
        }
        if let Some(same_site) = &self.same_site {
            flags.push(mask(&format!("SameSite={same_site}")));
        }
        let text = |value: &Option<String>| value.as_deref().map(&mask).unwrap_or_default();
        CookieColumns {
            name: mask(&self.name),
            value,
            domain: text(&self.domain),
            path: text(&self.path),
            expires: mask(&expires_text(
                self.expires.as_deref(),
                self.max_age.as_deref(),
            )),
            flags,
        }
    }
}

/// How many `Set-Cookie` headers `headers` has, the count on the Cookies sub-tab.
pub(crate) fn cookie_count(headers: &[(String, String)]) -> usize {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .count()
}

/// Every `Set-Cookie` header of `headers`, matched case-insensitively, in received order.
pub(crate) fn cookies(headers: &[(String, String)]) -> Vec<Cookie> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| match parse(value) {
            Some(cookie) => Cookie::Parsed(cookie),
            None => Cookie::Raw(value.clone()),
        })
        .collect()
}

/// `header` as RFC 6265 reads it: `name=value` then `;`-separated attributes, names
/// matched case-insensitively and unknown ones ignored. `None` when the first pair has no
/// `=` or an empty name.
pub(crate) fn parse(header: &str) -> Option<SetCookie> {
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value);
    let mut cookie = SetCookie {
        name: name.to_owned(),
        value: value.to_owned(),
        ..SetCookie::default()
    };
    for attribute in parts {
        let (key, value) = match attribute.split_once('=') {
            Some((key, value)) => (key.trim(), Some(value.trim().to_owned())),
            None => (attribute.trim(), None),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => cookie.domain = value,
            "path" => cookie.path = value,
            "expires" => cookie.expires = value,
            "max-age" => cookie.max_age = value,
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            "samesite" => cookie.same_site = value,
            _ => {}
        }
    }
    Some(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_attribute_parses() {
        let cookie = parse(
            "sid=abc123; Domain=example.com; Path=/api; Expires=Wed, 21 Oct 2026 07:28:00 GMT; \
             Max-Age=3600; Secure; HttpOnly; SameSite=Strict",
        );
        assert_eq!(
            cookie,
            Some(SetCookie {
                name: "sid".to_owned(),
                value: "abc123".to_owned(),
                domain: Some("example.com".to_owned()),
                path: Some("/api".to_owned()),
                expires: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_owned()),
                max_age: Some("3600".to_owned()),
                secure: true,
                http_only: true,
                same_site: Some("Strict".to_owned()),
            })
        );
    }

    #[test]
    fn attribute_names_are_case_insensitive_and_values_unquoted() {
        let cookie = parse("theme=\"dark mode\"; secure; HTTPONLY; samesite=lax; PATH=/");
        assert_eq!(
            cookie,
            Some(SetCookie {
                name: "theme".to_owned(),
                value: "dark mode".to_owned(),
                path: Some("/".to_owned()),
                secure: true,
                http_only: true,
                same_site: Some("lax".to_owned()),
                ..SetCookie::default()
            })
        );
        let empty = parse("gone=; Max-Age=0");
        assert_eq!(empty.as_ref().map(|cookie| cookie.value.as_str()), Some(""));
        assert_eq!(
            empty.and_then(|cookie| cookie.max_age).as_deref(),
            Some("0")
        );
    }

    #[test]
    fn the_columns_split_domain_path_expires_and_flags() {
        let cookie = parse(
            "region=eu; Domain=.acme.com; Path=/; Expires=Mon, 30 Nov 2026 10:05:00 GMT; \
             HttpOnly; Secure; SameSite=Lax",
        );
        assert_eq!(
            cookie.map(|cookie| cookie.columns(str::to_owned)),
            Some(CookieColumns {
                name: "region".to_owned(),
                value: CookieValue::Shown("eu".to_owned()),
                domain: ".acme.com".to_owned(),
                path: "/".to_owned(),
                expires: "30 Nov 2026, 10:05 UTC".to_owned(),
                flags: vec![
                    "HttpOnly".to_owned(),
                    "Secure".to_owned(),
                    "SameSite=Lax".to_owned()
                ],
            })
        );
    }

    #[test]
    fn expires_reads_max_age_then_the_date_then_session() {
        assert_eq!(expires_text(None, None), "Session");
        assert_eq!(
            expires_text(Some("Wed, 21 Oct 2026 07:28:00 GMT"), None),
            "21 Oct 2026, 07:28 UTC"
        );
        assert_eq!(expires_text(Some("soon"), None), "soon");
        assert_eq!(
            expires_text(Some("Wed, 21 Oct 2026 07:28:00 GMT"), Some("60")),
            "Max-Age 60"
        );
    }

    #[test]
    fn a_value_with_a_secret_the_send_knows_is_hidden() {
        let masker = |text: &str| text.replace("tok-123", MASK);
        let secret = parse("sid=tok-123; Domain=api.tok-123.dev");
        let columns = secret.map(|cookie| cookie.columns(masker));
        assert_eq!(
            columns.as_ref().map(|columns| &columns.value),
            Some(&CookieValue::Secret)
        );
        assert_eq!(
            columns.map(|columns| columns.domain),
            Some(format!("api.{MASK}.dev")),
            "any other column is masked by text"
        );
        let recorded = parse(&format!("sid={MASK}")).map(|cookie| cookie.columns(str::to_owned));
        assert_eq!(
            recorded.map(|columns| columns.value),
            Some(CookieValue::Secret),
            "a value history stored masked"
        );
        let plain = parse("theme=dark").map(|cookie| cookie.columns(masker));
        assert_eq!(
            plain.map(|columns| columns.value),
            Some(CookieValue::Shown("dark".to_owned()))
        );
    }

    #[test]
    fn a_malformed_header_stays_raw() {
        assert_eq!(parse("no equals sign; Path=/"), None);
        assert_eq!(parse("=value"), None);
        let headers = vec![
            ("Set-Cookie".to_owned(), "a=1".to_owned()),
            ("content-type".to_owned(), "text/plain".to_owned()),
            ("set-cookie".to_owned(), "broken".to_owned()),
        ];
        assert_eq!(
            cookies(&headers),
            vec![
                Cookie::Parsed(SetCookie {
                    name: "a".to_owned(),
                    value: "1".to_owned(),
                    ..SetCookie::default()
                }),
                Cookie::Raw("broken".to_owned()),
            ]
        );
    }
}
