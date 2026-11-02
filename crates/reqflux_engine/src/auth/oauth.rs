//! OAuth 2.0 token acquisition (RFC 6749): client credentials, password, refresh token and
//! authorization code with PKCE. The token is kept in the secret store, never in a workspace
//! file, and refreshed before a send once it has expired.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqflux_core::model::{ClientAuthentication, OAuth2Auth, OAuth2Grant};
use reqflux_http::{HttpError, PreparedBody, PreparedRequest, RequestOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::AuthContext;
use super::loopback;
use crate::secrets::SecretKey;

/// A token is refreshed this long before it expires, so it does not expire in flight.
const EXPIRY_MARGIN: u64 = 30;

/// How long the authorization code grant waits for the browser to come back.
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(300);

/// An OAuth 2.0 token as the secret store keeps it, JSON in one entry. Its `Debug` output
/// shows no token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OAuthToken {
    /// The access token.
    pub(crate) access_token: String,
    /// The refresh token, when the server issued one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    /// When the access token expires, in seconds since the Unix epoch; `None` when the
    /// server did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<u64>,
}

impl std::fmt::Debug for OAuthToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthToken")
            .field("refresh_token", &self.refresh_token.is_some())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl OAuthToken {
    fn expired(&self, now: u64) -> bool {
        self.expires_at
            .is_some_and(|expires_at| now.saturating_add(EXPIRY_MARGIN) >= expires_at)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The access token to send: the stored one while it is valid, else a refreshed one, else a
/// new one obtained with the configured grant. A new token is stored before it is returned.
pub(super) async fn access_token(
    context: &AuthContext,
    config: &OAuth2Auth,
    key: &SecretKey,
    options: RequestOptions,
) -> Result<String, HttpError> {
    let stored = read(context, key);
    if let Some(stored) = &stored {
        if !stored.expired(unix_now()) {
            return Ok(stored.access_token.clone());
        }
        if let Some(refresh) = stored.refresh_token.as_deref() {
            match refresh_grant(context, config, refresh, options).await {
                Ok(mut token) => {
                    if token.refresh_token.is_none() {
                        token.refresh_token = Some(refresh.to_owned());
                    }
                    return Ok(store(context, key, token));
                }
                Err(error) => log::info!("the OAuth 2.0 token was not refreshed: {error}"),
            }
        }
    }
    let token = match config.grant_type {
        OAuth2Grant::ClientCredentials => {
            let form = vec![pair("grant_type", "client_credentials")];
            token_request(context, config, form, options).await?
        }
        OAuth2Grant::Password => {
            let form = vec![
                pair("grant_type", "password"),
                pair("username", &config.username),
                pair("password", &config.password),
            ];
            token_request(context, config, form, options).await?
        }
        OAuth2Grant::RefreshToken => {
            if config.refresh_token.is_empty() {
                return Err(HttpError::Auth(
                    "the OAuth 2.0 refresh token grant has no refresh token".to_owned(),
                ));
            }
            refresh_grant(context, config, &config.refresh_token, options).await?
        }
        OAuth2Grant::AuthorizationCode => {
            let authorization = tokio::time::timeout(
                AUTHORIZE_TIMEOUT,
                loopback::authorize(context.browser.as_ref(), config),
            )
            .await
            .map_err(|_| {
                HttpError::Auth("the browser did not complete the authorization".to_owned())
            })??;
            let form = vec![
                pair("grant_type", "authorization_code"),
                pair("code", &authorization.code),
                pair("redirect_uri", &authorization.redirect_uri),
                pair("code_verifier", &authorization.verifier),
            ];
            token_request(context, config, form, options).await?
        }
    };
    Ok(store(context, key, token))
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

async fn refresh_grant(
    context: &AuthContext,
    config: &OAuth2Auth,
    refresh: &str,
    options: RequestOptions,
) -> Result<OAuthToken, HttpError> {
    let form = vec![
        pair("grant_type", "refresh_token"),
        pair("refresh_token", refresh),
    ];
    token_request(context, config, form, options).await
}

fn read(context: &AuthContext, key: &SecretKey) -> Option<OAuthToken> {
    match context.secrets.get(key) {
        Ok(Some(text)) => serde_json::from_str(&text).ok(),
        Ok(None) => None,
        Err(error) => {
            log::warn!("the OAuth 2.0 token was not read from the keyring: {error}");
            None
        }
    }
}

/// Stores `token` and returns its access token. A store that cannot keep it is logged; the
/// token is still sent.
fn store(context: &AuthContext, key: &SecretKey, token: OAuthToken) -> String {
    match serde_json::to_string(&token) {
        Ok(text) => {
            if let Err(error) = context.secrets.set(key, &text) {
                log::warn!("the OAuth 2.0 token was not stored in the keyring: {error}");
            }
        }
        Err(error) => log::warn!("the OAuth 2.0 token was not serialized: {error}"),
    }
    token.access_token
}

/// Posts `form` to the token endpoint with the client authentication and the scope, and
/// reads the token from a JSON or form-encoded answer.
async fn token_request(
    context: &AuthContext,
    config: &OAuth2Auth,
    mut form: Vec<(String, String)>,
    options: RequestOptions,
) -> Result<OAuthToken, HttpError> {
    let mut headers = vec![pair("Accept", "application/json")];
    if !config.scope.is_empty() {
        form.push(pair("scope", &config.scope));
    }
    match config.client_auth {
        ClientAuthentication::Body => {
            if !config.client_id.is_empty() {
                form.push(pair("client_id", &config.client_id));
            }
            if !config.client_secret.is_empty() {
                form.push(pair("client_secret", &config.client_secret));
            }
        }
        ClientAuthentication::Header => {
            let encode = |text: &str| -> String {
                url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
            };
            let credentials = format!(
                "{}:{}",
                encode(&config.client_id),
                encode(&config.client_secret)
            );
            headers.push(pair(
                "Authorization",
                &format!("Basic {}", STANDARD.encode(credentials)),
            ));
        }
    }
    let request = PreparedRequest {
        method: "POST".to_owned(),
        url: config.token_url.clone(),
        headers,
        body: PreparedBody::Form(form),
        options: RequestOptions {
            follow_redirects: false,
            ..options
        },
    };
    let response = context
        .client
        .send(request, CancellationToken::new())
        .await?;
    let fields = fields(&response.body);
    let text = |key: &str| match fields.get(key) {
        Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
        _ => None,
    };
    if !(200..300).contains(&response.status) {
        let code = text("error").map_or_else(String::new, |code| format!(" ({code})"));
        return Err(HttpError::Auth(format!(
            "the token endpoint answered {}{code}",
            response.status
        )));
    }
    let access_token = text("access_token").ok_or_else(|| {
        HttpError::Auth("the token endpoint answer has no access_token".to_owned())
    })?;
    let expires_in = match fields.get("expires_in") {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.parse().ok(),
        _ => None,
    };
    Ok(OAuthToken {
        access_token,
        refresh_token: text("refresh_token"),
        expires_at: expires_in.map(|seconds| unix_now().saturating_add(seconds)),
    })
}

/// The fields of a token answer: a JSON object, or form-encoded pairs as some servers send.
fn fields(body: &[u8]) -> serde_json::Map<String, Value> {
    if let Ok(Value::Object(object)) = serde_json::from_slice(body) {
        return object;
    }
    url::form_urlencoded::parse(body)
        .map(|(key, value)| (key.into_owned(), Value::String(value.into_owned())))
        .collect()
}
