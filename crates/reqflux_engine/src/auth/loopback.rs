//! The authorization code grant with PKCE (RFC 7636, S256) through the system browser and a
//! loopback redirect on `127.0.0.1` with a random port (RFC 8252).

use std::fmt::Debug;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqflux_core::model::OAuth2Auth;
use reqflux_http::HttpError;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The path the authorization server redirects back to.
const CALLBACK: &str = "/callback";

/// The most bytes read from a callback request before its head must have ended.
const MAX_HEAD: usize = 16 * 1024;

/// Opens an authorization URL for the user. The app opens the system browser; a test drives
/// the redirect itself.
pub trait Browser: Debug + Send + Sync {
    /// Opens `url`.
    ///
    /// # Errors
    ///
    /// The URL could not be opened; the text says why without repeating the URL.
    fn open(&self, url: &str) -> Result<(), String>;
}

/// The system browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBrowser;

impl Browser for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        open::that_detached(url).map_err(|error| format!("the browser did not open: {error}"))
    }
}

/// What the browser came back with.
pub(super) struct Authorization {
    pub(super) code: String,
    pub(super) redirect_uri: String,
    pub(super) verifier: String,
}

/// URL-safe random text from `bytes` random bytes.
pub(super) fn random_text(bytes: usize) -> String {
    let random: Vec<u8> = (0..bytes).map(|_| rand::random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(random)
}

/// Listens on a random loopback port, opens the authorization URL and waits for the
/// redirect whose `state` matches. A request to another path is answered 404 and the wait
/// goes on; a wrong `state` or an `error` ends it.
pub(super) async fn authorize(
    browser: &dyn Browser,
    config: &OAuth2Auth,
) -> Result<Authorization, HttpError> {
    let failed = |reason: String| HttpError::Auth(reason);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| failed(format!("the loopback redirect did not start: {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| failed(format!("the loopback redirect did not start: {error}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK}");
    let verifier = random_text(32);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text(16);

    let mut url = url::Url::parse(&config.auth_url)
        .map_err(|_| failed("the OAuth 2.0 authorization URL is not valid".to_owned()))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if !config.scope.is_empty() {
            query.append_pair("scope", &config.scope);
        }
    }
    browser.open(url.as_str()).map_err(failed)?;

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|error| failed(format!("the loopback redirect failed: {error}")))?;
        if let Some(outcome) = callback(stream, &state).await {
            return outcome.map(|code| Authorization {
                code,
                redirect_uri,
                verifier,
            });
        }
    }
}

/// Answers one loopback request: `None` when it is not the callback, else the code or why
/// the authorization failed.
async fn callback(mut stream: TcpStream, state: &str) -> Option<Result<String, HttpError>> {
    let head = read_head(&mut stream).await?;
    let target = head.lines().next()?.split_whitespace().nth(1)?.to_owned();
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        respond(&mut stream, "400 Bad Request", "Bad request.").await;
        return None;
    };
    if url.path() != CALLBACK {
        respond(&mut stream, "404 Not Found", "Not found.").await;
        return None;
    }
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    let outcome = if param("state").as_deref() != Some(state) {
        Err("the authorization redirect has the wrong state".to_owned())
    } else if let Some(error) = param("error") {
        Err(format!("the authorization server answered {error}"))
    } else {
        param("code").ok_or_else(|| "the authorization redirect has no code".to_owned())
    };
    match &outcome {
        Ok(_) => {
            respond(
                &mut stream,
                "200 OK",
                "ReqFlux is authorized. You can close this window.",
            )
            .await;
        }
        Err(_) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "ReqFlux was not authorized. You can close this window.",
            )
            .await;
        }
    }
    Some(outcome.map_err(HttpError::Auth))
}

async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        if head.len() > MAX_HEAD {
            return None;
        }
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            break;
        }
        head.extend(buffer.get(..read)?);
    }
    String::from_utf8(head).ok()
}

async fn respond(stream: &mut TcpStream, status: &str, text: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    if let Err(error) = stream.write_all(response.as_bytes()).await {
        log::debug!("the loopback redirect answer was not written: {error}");
    }
}
