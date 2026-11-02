//! The Token row of an OAuth 2.0 auth without GPUI: what the badge, the time and the hint say
//! for the state of the kept token, which the engine reads without the token.

use chrono::{DateTime, FixedOffset, Utc};
use reqflux_core::model::OAuth2Auth;
use reqflux_engine::OAuthTokenState;

/// The tone of the token badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenTone {
    Valid,
    Expired,
    Missing,
}

/// What the Token row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenText {
    pub(crate) badge: &'static str,
    pub(crate) tone: TokenTone,
    /// The expiry, after the badge: `until 14:32 · 52 min left`, `at 09:10 today`.
    pub(crate) time: Option<String>,
    pub(crate) hint: String,
    /// Get new token is the primary action: there is no token to send.
    pub(crate) get_is_primary: bool,
    /// A token is kept, so Clear is offered.
    pub(crate) clearable: bool,
}

/// `HH:MM` of `at`, seconds since the Unix epoch, at `offset` when it falls on the day of
/// `now` there, else `Nov 3, HH:MM`; and whether it is that day.
fn clock(at: u64, now: u64, offset: FixedOffset) -> (String, bool) {
    let local = |seconds: u64| {
        i64::try_from(seconds)
            .ok()
            .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
            .map(|moment| moment.with_timezone(&offset))
    };
    match (local(at), local(now)) {
        (Some(at), Some(now)) if at.date_naive() == now.date_naive() => {
            (at.format("%H:%M").to_string(), true)
        }
        (Some(at), Some(_)) => (at.format("%b %-d, %H:%M").to_string(), false),
        _ => (String::new(), false),
    }
}

/// How long is left until `at` from `now`: `52 min left`, `3 h left`.
fn left(at: u64, now: u64) -> String {
    let minutes = at.saturating_sub(now) / 60;
    if minutes < 60 {
        format!("{minutes} min left")
    } else {
        format!("{} h left", minutes / 60)
    }
}

/// The Token row of `state` for the auth `oauth`, seen at `now` (seconds since the Unix
/// epoch) in the time zone at `offset`.
pub(crate) fn token_text(
    state: OAuthTokenState,
    oauth: &OAuth2Auth,
    now: u64,
    offset: FixedOffset,
) -> TokenText {
    match state {
        OAuthTokenState::None => TokenText {
            badge: "No token",
            tone: TokenTone::Missing,
            time: None,
            hint: "Get a token before sending.".to_owned(),
            get_is_primary: true,
            clearable: false,
        },
        OAuthTokenState::Valid { expires_at } => {
            let time = expires_at.map(|at| {
                let (clock, _) = clock(at, now, offset);
                format!("until {clock} · {}", left(at, now))
            });
            let mut parts = vec![oauth.scheme().to_owned()];
            if !oauth.scope.is_empty() {
                parts.push(oauth.scope.clone());
            }
            parts.push("in the keyring".to_owned());
            TokenText {
                badge: "Valid",
                tone: TokenTone::Valid,
                time: Some(time.unwrap_or_else(|| "with no expiry".to_owned())),
                hint: parts.join(" · "),
                get_is_primary: false,
                clearable: true,
            }
        }
        OAuthTokenState::Expired {
            expires_at,
            refreshable,
        } => TokenText {
            badge: "Expired",
            tone: TokenTone::Expired,
            time: expires_at.map(|at| {
                let (clock, today) = clock(at, now, offset);
                if today {
                    format!("at {clock} today")
                } else {
                    format!("at {clock}")
                }
            }),
            hint: if refreshable {
                "A refresh token is kept; Get new token uses it first.".to_owned()
            } else {
                "Get a new token before sending.".to_owned()
            },
            get_is_primary: true,
            clearable: true,
        },
    }
}

/// The host of `url` as written, for `Sign in at <host>`; the whole text when it has no
/// scheme.
pub(crate) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-02T13:40:00Z.
    const NOW: u64 = 1_793_626_800;

    fn utc() -> FixedOffset {
        match FixedOffset::east_opt(0) {
            Some(offset) => offset,
            None => panic!("offset"),
        }
    }

    fn oauth() -> OAuth2Auth {
        OAuth2Auth {
            scope: "invoices:read invoices:write".to_owned(),
            ..OAuth2Auth::default()
        }
    }

    #[test]
    fn a_valid_token_shows_its_expiry_and_time_left() {
        let text = token_text(
            OAuthTokenState::Valid {
                expires_at: Some(NOW + 52 * 60),
            },
            &oauth(),
            NOW,
            utc(),
        );
        assert_eq!(text.badge, "Valid");
        assert_eq!(text.time.as_deref(), Some("until 14:32 · 52 min left"));
        let later = token_text(
            OAuthTokenState::Valid {
                expires_at: Some(NOW + 20 * 3600),
            },
            &oauth(),
            NOW,
            utc(),
        );
        assert_eq!(
            later.time.as_deref(),
            Some("until Nov 3, 09:40 · 20 h left")
        );
        assert_eq!(
            text.hint,
            "Bearer · invoices:read invoices:write · in the keyring"
        );
        assert!(!text.get_is_primary);
        assert!(text.clearable);
    }

    #[test]
    fn no_token_makes_get_new_token_primary() {
        let text = token_text(OAuthTokenState::None, &oauth(), NOW, utc());
        assert_eq!(
            (text.badge, text.time, text.hint.as_str()),
            ("No token", None, "Get a token before sending.")
        );
        assert!(text.get_is_primary);
        assert!(!text.clearable);
    }

    #[test]
    fn an_expired_token_shows_when_and_whether_it_refreshes() {
        let text = token_text(
            OAuthTokenState::Expired {
                expires_at: Some(NOW - 4 * 3600 - 30 * 60),
                refreshable: true,
            },
            &oauth(),
            NOW,
            utc(),
        );
        assert_eq!(text.badge, "Expired");
        assert_eq!(text.time.as_deref(), Some("at 09:10 today"));
        assert_eq!(
            text.hint,
            "A refresh token is kept; Get new token uses it first."
        );
        assert!(text.get_is_primary && text.clearable);
        let refused = token_text(
            OAuthTokenState::Expired {
                expires_at: None,
                refreshable: false,
            },
            &oauth(),
            NOW,
            utc(),
        );
        assert_eq!(refused.time, None);
        assert_eq!(refused.hint, "Get a new token before sending.");
    }

    #[test]
    fn the_host_is_read_as_written() {
        assert_eq!(
            host("https://accounts.acme.dev/oauth/authorize"),
            "accounts.acme.dev"
        );
        assert_eq!(host("{{auth_url}}/authorize"), "{{auth_url}}");
    }
}
