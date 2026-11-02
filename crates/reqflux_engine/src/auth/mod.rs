//! Auth that needs more than a header from the fields (v1-features S3): the digest answer to
//! a 401 challenge, AWS SigV4 signing at send time, and the OAuth 2.0 token the engine
//! acquires, keeps in the secret store and refreshes.
//!
//! Preparation resolves the fields into a [`PendingAuth`]; [`Engine::send`](crate::Engine::send)
//! signs SigV4 before it records the request, and the send task gets the OAuth 2.0 token
//! first and answers a digest challenge once.

mod loopback;
mod oauth;

use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use reqflux_core::model::{OAuth2Auth, TokenLocation};
use reqflux_http::auth::digest::{self, DigestHash, DigestInput};
use reqflux_http::auth::sigv4::{self, AwsCredentials};
use reqflux_http::{HttpClient, HttpError, PreparedRequest, RedirectRules, Response};
use tokio_util::sync::CancellationToken;

use crate::secrets::{SecretKey, SecretStore};
use crate::sent::MASK;

pub use loopback::{Browser, SystemBrowser};

/// The resolved auth a send completes. Its `Debug` output names the type only.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum PendingAuth {
    /// Answer a `401` digest challenge once.
    Digest {
        username: String,
        password: String,
        fallback: DigestHash,
    },
    /// Sign with AWS SigV4.
    AwsV4 {
        access_key: String,
        secret_key: String,
        session_token: String,
        region: String,
        service: String,
    },
    /// Send the OAuth 2.0 token kept under `token`, acquiring it first when needed.
    OAuth2 {
        config: Box<OAuth2Auth>,
        token: SecretKey,
    },
}

impl fmt::Debug for PendingAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Digest { .. } => "Digest",
            Self::AwsV4 { .. } => "AwsV4",
            Self::OAuth2 { .. } => "OAuth2",
        };
        formatter.write_str(kind)
    }
}

/// What the send task needs to complete a [`PendingAuth`].
#[derive(Debug, Clone)]
pub(crate) struct AuthContext {
    pub(crate) client: HttpClient,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) browser: Arc<dyn Browser>,
}

/// `request` as history shows it before the send: an OAuth 2.0 token where it will go, as
/// the mask, since the token is only known once the send starts.
pub(crate) fn shown(request: &PreparedRequest, auth: Option<&PendingAuth>) -> PreparedRequest {
    let mut shown = request.clone();
    if let Some(PendingAuth::OAuth2 { config, .. }) = auth {
        add_token(&mut shown, config, MASK);
    }
    shown
}

/// Signs `request` with AWS SigV4 now, when that is its auth.
///
/// # Errors
///
/// The URL does not parse or has no host.
pub(crate) fn sign_now(
    request: &mut PreparedRequest,
    auth: Option<&PendingAuth>,
) -> Result<(), HttpError> {
    let Some(PendingAuth::AwsV4 {
        access_key,
        secret_key,
        session_token,
        region,
        service,
    }) = auth
    else {
        return Ok(());
    };
    let credentials = AwsCredentials {
        access_key,
        secret_key,
        session_token: Some(session_token.as_str()).filter(|token| !token.is_empty()),
        region,
        service,
    };
    sigv4::sign(request, &credentials, SystemTime::now())
}

/// Sends `request`, completing `auth` around it: the OAuth 2.0 token is added first, and a
/// digest challenge in a `401` is answered with one more send. A `401` to an OAuth 2.0
/// token drops it; when the token was a stored one, the send retries once with a new one.
pub(crate) async fn send(
    context: AuthContext,
    request: PreparedRequest,
    auth: Option<PendingAuth>,
    cancel: CancellationToken,
    rules: RedirectRules,
) -> Result<Response, HttpError> {
    match auth {
        Some(PendingAuth::OAuth2 { config, token }) => {
            let mut retried = false;
            loop {
                let (access, origin) = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(HttpError::Cancelled),
                    access = oauth::access_token(&context, &config, &token, request.options) => access?,
                };
                let mut attempt = request.clone();
                add_token(&mut attempt, &config, &access);
                let result = context
                    .client
                    .send_with(attempt, cancel.clone(), rules.clone())
                    .await;
                if !matches!(&result, Ok(response) if response.status == 401) {
                    return scrub(result, &access);
                }
                oauth::drop_access_token(&context, &token);
                if origin == oauth::Origin::New || retried {
                    return scrub(result, &access);
                }
                retried = true;
            }
        }
        Some(PendingAuth::Digest {
            username,
            password,
            fallback,
        }) => {
            let retry = request.clone();
            let first = context
                .client
                .send_with(request, cancel.clone(), rules.clone())
                .await?;
            if first.status != 401 {
                return Ok(first);
            }
            let Some(challenge) = digest::challenge(&first.headers) else {
                return Ok(first);
            };
            let uri = request_target(&retry.url);
            let cnonce = loopback::random_text(16);
            let input = DigestInput {
                method: &retry.method,
                uri: &uri,
                username: &username,
                password: &password,
                fallback,
                cnonce: &cnonce,
                nc: 1,
            };
            let header = digest::authorization(&challenge, &input)
                .map_err(|error| HttpError::Auth(error.to_string()))?;
            let mut retry = retry;
            retry.headers.push(("Authorization".to_owned(), header));
            context.client.send_with(retry, cancel, rules).await
        }
        Some(PendingAuth::AwsV4 { .. }) | None => {
            context.client.send_with(request, cancel, rules).await
        }
    }
}

/// The path and query of `url`, the digest `uri`.
fn request_target(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => match parsed.query() {
            Some(query) => format!("{}?{query}", parsed.path()),
            None => parsed.path().to_owned(),
        },
        Err(_) => "/".to_owned(),
    }
}

fn add_token(request: &mut PreparedRequest, config: &OAuth2Auth, token: &str) {
    match config.token_location {
        TokenLocation::Header => {
            if !request
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            {
                request.headers.push((
                    "Authorization".to_owned(),
                    format!("{} {token}", config.scheme()),
                ));
            }
        }
        TokenLocation::Query => {
            let encoded = url::form_urlencoded::Serializer::new(String::new())
                .append_pair(config.query_name(), token)
                .finish();
            let separator = if request.url.contains('?') { '&' } else { '?' };
            request.url = format!("{}{separator}{encoded}", request.url);
        }
    }
}

/// Masks the token in what comes back: a query token is in the final URL and can be in a
/// transport error.
fn scrub(result: Result<Response, HttpError>, token: &str) -> Result<Response, HttpError> {
    let encoded: String = url::form_urlencoded::byte_serialize(token.as_bytes()).collect();
    let mask = |text: String| text.replace(token, MASK).replace(&encoded, MASK);
    match result {
        Ok(mut response) => {
            response.final_url = mask(response.final_url);
            Ok(response)
        }
        Err(HttpError::InvalidUrl { url, reason }) => Err(HttpError::InvalidUrl {
            url: mask(url),
            reason: mask(reason),
        }),
        Err(HttpError::Connect { host, reason }) => Err(HttpError::Connect {
            host,
            reason: mask(reason),
        }),
        Err(HttpError::RedirectRefused { url, reason }) => Err(HttpError::RedirectRefused {
            url: mask(url),
            reason: mask(reason),
        }),
        Err(HttpError::RedirectSecretBody { url }) => {
            Err(HttpError::RedirectSecretBody { url: mask(url) })
        }
        Err(HttpError::Request(text)) => Err(HttpError::Request(mask(text))),
        Err(HttpError::Tls(text)) => Err(HttpError::Tls(mask(text))),
        Err(other) => Err(other),
    }
}
