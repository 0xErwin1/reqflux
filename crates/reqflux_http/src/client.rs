//! The client that sends a [`PreparedRequest`].

use std::collections::HashMap;
use std::error::Error as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::multipart::{Form, Part};
use reqwest::{Method, RequestBuilder, Version};
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::error::HttpError;
use crate::redirect;
use crate::request::{MultipartValue, PreparedBody, PreparedRequest, RequestOptions};
use crate::response::Response;

/// The maximum number of redirects followed for one request.
const MAX_REDIRECTS: usize = 10;

/// A check consulted for every redirect target before it is followed: the refusal text when
/// the redirect may not be followed. The client does not interpret it.
pub type RedirectCheck = Arc<dyn Fn(&Url) -> Result<(), String> + Send + Sync>;

/// Replaces every secret value of a send in a text. A header whose name or value it changes
/// holds a secret.
pub type SecretMask = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// What a send applies to the redirects it follows.
#[derive(Clone, Default)]
pub struct RedirectRules {
    /// Consulted for every redirect target before it is followed.
    pub check: Option<RedirectCheck>,
    /// The send's secrets: a cross-origin hop drops the headers holding one.
    pub mask: Option<SecretMask>,
}

impl std::fmt::Debug for RedirectRules {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedirectRules")
            .field("check", &self.check.is_some())
            .field("mask", &self.mask.is_some())
            .finish()
    }
}

/// Sends requests, reusing one `reqwest` client per verify TLS setting.
///
/// Cloning is cheap and clones share the cache. Requests must run inside a Tokio runtime.
#[derive(Debug, Clone, Default)]
pub struct HttpClient {
    clients: Arc<Mutex<HashMap<ClientKey, reqwest::Client>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClientKey {
    verify_tls: bool,
}

impl HttpClient {
    /// A client with an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends `request` and reads the whole response body.
    ///
    /// Cancelling `cancel` aborts only this request with [`HttpError::Cancelled`].
    pub async fn send(
        &self,
        request: PreparedRequest,
        cancel: CancellationToken,
    ) -> Result<Response, HttpError> {
        self.send_checked(request, cancel, None).await
    }

    /// [`HttpClient::send`] with `check` consulted for every redirect target before it is
    /// followed. A refused redirect stops the chain with [`HttpError::RedirectRefused`] and
    /// nothing is sent to its target. Without a check this is [`HttpClient::send`].
    pub async fn send_checked(
        &self,
        request: PreparedRequest,
        cancel: CancellationToken,
        check: Option<RedirectCheck>,
    ) -> Result<Response, HttpError> {
        let rules = RedirectRules { check, mask: None };
        self.send_with(request, cancel, rules).await
    }

    /// [`HttpClient::send`] applying `rules` to every redirect it follows, at most
    /// [`MAX_REDIRECTS`]. A hop to another origin (scheme, host or port) drops the
    /// `Authorization`, `Cookie`, `Proxy-Authorization` and `WWW-Authenticate` headers and every
    /// header holding a secret of `rules.mask`; a 307 or 308 to another origin whose body holds
    /// one stops with [`HttpError::RedirectSecretBody`]. A same-origin hop keeps every header.
    /// The timeout bounds the whole chain.
    pub async fn send_with(
        &self,
        request: PreparedRequest,
        cancel: CancellationToken,
        rules: RedirectRules,
    ) -> Result<Response, HttpError> {
        let timeout = request.options.timeout;
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(HttpError::Cancelled),
            result = tokio::time::timeout(timeout, self.execute(request, rules)) => {
                result.unwrap_or(Err(HttpError::Timeout(timeout.as_millis())))
            }
        }
    }

    async fn execute(
        &self,
        request: PreparedRequest,
        rules: RedirectRules,
    ) -> Result<Response, HttpError> {
        let mut url = Url::parse(&request.url).map_err(|error| HttpError::InvalidUrl {
            url: request.url.clone(),
            reason: error.to_string(),
        })?;
        let mut method = Method::from_bytes(request.method.as_bytes())
            .map_err(|_| HttpError::InvalidMethod(request.method.clone()))?;
        let mut headers = header_map(&request.headers)?;
        let follow = request.options.follow_redirects;
        let timeout = request.options.timeout;
        let client = self.client(request.options)?;
        let mut body = request.body;
        let mut hops = 0;
        let started = Instant::now();
        loop {
            let replay = if follow {
                redirect::replayable(&body)
            } else {
                None
            };
            let has_content_type = headers.contains_key(CONTENT_TYPE);
            let builder = client
                .request(method.clone(), url.clone())
                .headers(headers.clone());
            let builder = with_body(builder, body, has_content_type).await?;
            let response = builder
                .send()
                .await
                .map_err(|error| classify(&error, &url, timeout))?;
            let next = if follow {
                redirect::target(&response, &url)
            } else {
                None
            };
            let Some(next) = next else {
                return read(response, &url, started, timeout).await;
            };
            let keeps_body = redirect::keeps_body(response.status());
            body = match (keeps_body, replay) {
                (true, Some(replay)) => replay,
                (true, None) => return read(response, &url, started, timeout).await,
                (false, _) => {
                    redirect::to_get(&mut method, &mut headers);
                    PreparedBody::None
                }
            };
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(HttpError::Request(format!(
                    "error following redirect for url ({next}): too many redirects"
                )));
            }
            if let Some(check) = &rules.check {
                check(&next).map_err(|reason| HttpError::RedirectRefused {
                    url: next.to_string(),
                    reason,
                })?;
            }
            redirect::set_referer(&mut headers, &url, &next);
            if next.origin() != url.origin() {
                if let Some(mask) = &rules.mask
                    && redirect::body_holds_secret(&body, mask)
                {
                    return Err(HttpError::RedirectSecretBody {
                        url: mask(next.as_str()),
                    });
                }
                redirect::strip_cross_origin(&mut headers, rules.mask.as_ref());
            }
            url = next;
        }
    }

    fn client(&self, options: RequestOptions) -> Result<reqwest::Client, HttpError> {
        let key = ClientKey {
            verify_tls: options.verify_tls,
        };
        let mut clients = self.clients.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        // Redirects are followed by `execute`, which can change headers between hops.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .tls_danger_accept_invalid_certs(!key.verify_tls)
            .build()
            .map_err(|error| HttpError::Client(error_chain(&error)))?;
        clients.insert(key, client.clone());
        Ok(client)
    }
}

/// The final response of a send, its whole body read.
async fn read(
    response: reqwest::Response,
    url: &Url,
    started: Instant,
    timeout: Duration,
) -> Result<Response, HttpError> {
    let status = response.status();
    let version = version_text(response.version());
    let final_url = response.url().to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|error| classify(&error, url, timeout))?;
    Ok(Response {
        status: status.as_u16(),
        reason: status.canonical_reason().map(str::to_owned),
        version,
        headers,
        body: body.to_vec(),
        final_url,
        elapsed: started.elapsed(),
    })
}

fn header_map(headers: &[(String, String)]) -> Result<HeaderMap, HttpError> {
    let mut map = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        let invalid = |reason: String| HttpError::InvalidHeader {
            name: name.clone(),
            reason,
        };
        valid_header_text(name, "name").map_err(invalid)?;
        valid_header_text(value, "value").map_err(invalid)?;
        let header_name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|error| invalid(error.to_string()))?;
        let header_value =
            HeaderValue::from_str(value).map_err(|error| invalid(error.to_string()))?;
        map.append(header_name, header_value);
    }
    Ok(map)
}

/// Rejects CR, LF, NUL and non-ASCII bytes before the request is built.
///
/// A CR or LF would smuggle extra headers into the request (header injection); a NUL or a
/// non-ASCII byte would either fail deep inside the transport or be sent as raw bytes, which
/// is never what a header means. Encoded values, such as RFC 2047 words, are the caller's job
/// and pass unchanged.
fn valid_header_text(text: &str, part: &str) -> Result<(), String> {
    for byte in text.bytes() {
        let reason = match byte {
            b'\r' | b'\n' => "contains CR or LF".to_owned(),
            0 => "contains a NUL byte".to_owned(),
            128.. => "has a non-ASCII character".to_owned(),
            _ => continue,
        };
        return Err(format!("the header {part} {reason}"));
    }
    Ok(())
}

async fn with_body(
    builder: RequestBuilder,
    body: PreparedBody,
    has_content_type: bool,
) -> Result<RequestBuilder, HttpError> {
    Ok(match body {
        PreparedBody::None => builder,
        PreparedBody::Bytes { content_type, data } => {
            let builder = match content_type {
                Some(content_type) if !has_content_type => {
                    builder.header(CONTENT_TYPE, content_type)
                }
                _ => builder,
            };
            builder.body(data)
        }
        PreparedBody::Form(fields) => builder.form(&fields),
        PreparedBody::Multipart(parts) => {
            let mut form = Form::new();
            for part in parts {
                form = match part.value {
                    MultipartValue::Text(text) => form.text(part.name, text),
                    MultipartValue::File(path) => {
                        let file = Part::file(&path)
                            .await
                            .map_err(|source| HttpError::File { path, source })?;
                        form.part(part.name, file)
                    }
                };
            }
            builder.multipart(form)
        }
        PreparedBody::File(path) => {
            let file = tokio::fs::File::open(&path)
                .await
                .map_err(|source| HttpError::File { path, source })?;
            builder.body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
        }
    })
}

/// Maps a `reqwest` failure to the S2 messages.
fn classify(error: &reqwest::Error, url: &Url, timeout: Duration) -> HttpError {
    if error.is_timeout() {
        return HttpError::Timeout(timeout.as_millis());
    }
    if error.is_connect() {
        if is_tls(error) {
            return HttpError::Tls(root_cause(error));
        }
        return HttpError::Connect {
            host: url.host_str().unwrap_or_default().to_owned(),
            reason: root_cause(error),
        };
    }
    HttpError::Request(error_chain(error))
}

/// Whether the error chain comes from the TLS layer. `reqwest` does not expose a typed TLS
/// error, so this matches the rustls messages, which always name a certificate or TLS.
fn is_tls(error: &reqwest::Error) -> bool {
    let mut source = error.source();
    while let Some(current) = source {
        let message = current.to_string().to_lowercase();
        if message.contains("certificate") || message.contains("tls") {
            return true;
        }
        source = current.source();
    }
    false
}

/// The message of the innermost error, which carries the actual reason.
fn root_cause(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(current) = source {
        message = current.to_string();
        source = current.source();
    }
    message
}

/// The error and its sources joined with `: `.
fn error_chain(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(current) = source {
        message.push_str(": ");
        message.push_str(&current.to_string());
        source = current.source();
    }
    message
}

fn version_text(version: Version) -> String {
    match version {
        Version::HTTP_09 => "0.9",
        Version::HTTP_10 => "1.0",
        Version::HTTP_11 => "1.1",
        Version::HTTP_2 => "2",
        Version::HTTP_3 => "3",
        _ => "unknown",
    }
    .to_owned()
}
