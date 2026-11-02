//! The options of a curl command, read into a [`Parsed`] request.

use super::body::{self, Data};
use super::{CurlError, Parsed};
use crate::model::{Auth, Body, FormPart, KeyValue, Method, Settings};
use crate::query;

/// What an option does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Request,
    Header,
    Data(DataKind),
    Json,
    Form {
        literal: bool,
    },
    User,
    Bearer,
    Cookie,
    UserAgent,
    Referer,
    Url,
    MaxTime,
    Insecure,
    Location,
    Head,
    Get,
    /// Changes only curl's own output; accepted silently.
    Quiet,
    /// Changes the request but has no place in a ReqFlux request; a warning.
    Unsupported {
        value: bool,
    },
}

/// How `-d` and its variants read their value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DataKind {
    /// `-d`, `--data`, `--data-ascii`, `--data-binary`: `@file` reads a file.
    Data,
    /// `--data-raw`: as written.
    Raw,
    /// `--data-urlencode`: encodes the content.
    Urlencode,
}

/// The long name of a short option.
fn long_name(short: char) -> Option<&'static str> {
    Some(match short {
        'X' => "request",
        'H' => "header",
        'd' => "data",
        'F' => "form",
        'u' => "user",
        'b' => "cookie",
        'A' => "user-agent",
        'e' => "referer",
        'm' => "max-time",
        'k' => "insecure",
        'L' => "location",
        'I' => "head",
        'G' => "get",
        's' => "silent",
        'S' => "show-error",
        'v' => "verbose",
        'i' => "include",
        'N' => "no-buffer",
        'f' => "fail",
        '#' => "progress-bar",
        'g' => "globoff",
        'x' => "proxy",
        'o' => "output",
        'O' => "remote-name",
        'E' => "cert",
        'K' => "config",
        'D' => "dump-header",
        'c' => "cookie-jar",
        'w' => "write-out",
        'T' => "upload-file",
        'r' => "range",
        'U' => "proxy-user",
        'Y' => "speed-limit",
        'y' => "speed-time",
        'z' => "time-cond",
        'C' => "continue-at",
        '0' => "http1.0",
        '4' => "ipv4",
        '6' => "ipv6",
        'j' => "junk-session-cookies",
        _ => return None,
    })
}

fn classify(long: &str) -> Option<Opt> {
    Some(match long {
        "request" => Opt::Request,
        "header" => Opt::Header,
        "data" | "data-ascii" | "data-binary" => Opt::Data(DataKind::Data),
        "data-raw" => Opt::Data(DataKind::Raw),
        "data-urlencode" => Opt::Data(DataKind::Urlencode),
        "json" => Opt::Json,
        "form" => Opt::Form { literal: false },
        "form-string" => Opt::Form { literal: true },
        "user" => Opt::User,
        "oauth2-bearer" => Opt::Bearer,
        "cookie" => Opt::Cookie,
        "user-agent" => Opt::UserAgent,
        "referer" => Opt::Referer,
        "url" => Opt::Url,
        "max-time" => Opt::MaxTime,
        "insecure" => Opt::Insecure,
        "location" | "location-trusted" => Opt::Location,
        "head" => Opt::Head,
        "get" => Opt::Get,
        "silent" | "show-error" | "verbose" | "include" | "no-buffer" | "fail"
        | "fail-with-body" | "progress-bar" | "globoff" | "compressed" | "no-progress-meter"
        | "raw" => Opt::Quiet,
        "proxy"
        | "output"
        | "cert"
        | "key"
        | "cacert"
        | "capath"
        | "config"
        | "dump-header"
        | "cookie-jar"
        | "write-out"
        | "upload-file"
        | "range"
        | "proxy-user"
        | "speed-limit"
        | "speed-time"
        | "time-cond"
        | "continue-at"
        | "connect-timeout"
        | "retry"
        | "retry-delay"
        | "retry-max-time"
        | "max-redirs"
        | "resolve"
        | "connect-to"
        | "interface"
        | "limit-rate"
        | "unix-socket"
        | "abstract-unix-socket"
        | "local-port"
        | "ciphers"
        | "tls-max"
        | "max-filesize"
        | "trace"
        | "trace-ascii"
        | "stderr"
        | "aws-sigv4"
        | "request-target"
        | "noproxy"
        | "preproxy"
        | "proxy-header"
        | "cert-type"
        | "key-type"
        | "pass"
        | "pinnedpubkey"
        | "dns-servers"
        | "expect100-timeout" => Opt::Unsupported { value: true },
        "remote-name"
        | "http1.0"
        | "http1.1"
        | "http2"
        | "http2-prior-knowledge"
        | "http3"
        | "ipv4"
        | "ipv6"
        | "junk-session-cookies"
        | "digest"
        | "ntlm"
        | "negotiate"
        | "anyauth"
        | "path-as-is"
        | "proxy-insecure"
        | "tcp-nodelay"
        | "tr-encoding"
        | "remote-header-name"
        | "create-dirs" => Opt::Unsupported { value: false },
        _ => return None,
    })
}

const fn takes_value(opt: Opt) -> bool {
    !matches!(
        opt,
        Opt::Insecure
            | Opt::Location
            | Opt::Head
            | Opt::Get
            | Opt::Quiet
            | Opt::Unsupported { value: false }
    )
}

#[derive(Default)]
struct Builder {
    method: Option<String>,
    head: bool,
    get: bool,
    json: bool,
    urls: Vec<String>,
    headers: Vec<KeyValue>,
    auth: Option<Auth>,
    data: Vec<Data>,
    form: Vec<FormPart>,
    settings: Settings,
    warnings: Vec<String>,
}

/// Reads the arguments after `curl`.
pub(super) fn read(args: &[String]) -> Result<Parsed, CurlError> {
    let mut builder = Builder::default();
    let mut args = args.iter();
    let mut options_done = false;
    while let Some(arg) = args.next() {
        if options_done || !arg.starts_with('-') || arg == "-" {
            builder.urls.push(arg.clone());
        } else if arg == "--" {
            options_done = true;
        } else if let Some(long) = arg.strip_prefix("--") {
            let Some(opt) = classify(long) else {
                // Only the name: an `--option=value` value can be a credential, and the
                // warnings are shown to the user.
                let name = long.split_once('=').map_or(long, |(name, _)| name);
                builder
                    .warnings
                    .push(format!("unknown option --{name} was ignored"));
                continue;
            };
            let value = if takes_value(opt) {
                Some(
                    args.next()
                        .ok_or_else(|| CurlError::MissingValue(arg.clone()))?,
                )
            } else {
                None
            };
            builder.apply(opt, long, value.map(String::as_str))?;
        } else {
            let cluster: Vec<char> = arg.chars().skip(1).collect();
            let mut at = 0;
            while let Some(&short) = cluster.get(at) {
                at += 1;
                let Some((long, opt)) =
                    long_name(short).and_then(|long| Some((long, classify(long)?)))
                else {
                    builder
                        .warnings
                        .push(format!("unknown option -{short} was ignored"));
                    continue;
                };
                if !takes_value(opt) {
                    builder.apply(opt, long, None)?;
                    continue;
                }
                let rest: String = cluster.get(at..).unwrap_or_default().iter().collect();
                let value = if rest.is_empty() {
                    args.next()
                        .cloned()
                        .ok_or_else(|| CurlError::MissingValue(format!("-{short}")))?
                } else {
                    rest
                };
                builder.apply(opt, long, Some(&value))?;
                break;
            }
        }
    }
    builder.finish()
}

impl Builder {
    fn apply(&mut self, opt: Opt, long: &str, value: Option<&str>) -> Result<(), CurlError> {
        let value = value.unwrap_or_default();
        match opt {
            Opt::Request => {
                Method::new(value)?;
                self.method = Some(value.to_owned());
            }
            Opt::Header => self.header(value),
            Opt::Data(kind) => self.data.push(body::data(kind, value)),
            Opt::Json => {
                self.json = true;
                self.data.push(body::data(DataKind::Data, value));
            }
            Opt::Form { literal } => {
                let (part, warnings) = body::form(value, literal);
                self.form.extend(part);
                self.warnings.extend(warnings);
            }
            Opt::User => {
                let (username, password) = match value.split_once(':') {
                    Some((username, password)) => (username, password),
                    None => {
                        self.warnings.push(
                            "-u without a password: curl would prompt for it; it is empty"
                                .to_owned(),
                        );
                        (value, "")
                    }
                };
                self.auth = Some(Auth::Basic {
                    username: username.to_owned(),
                    password: password.to_owned(),
                });
            }
            Opt::Bearer => {
                self.auth = Some(Auth::Bearer {
                    token: value.to_owned(),
                });
            }
            Opt::Cookie if value.contains('=') => {
                self.headers.push(KeyValue::new("Cookie", value));
            }
            Opt::Cookie => self
                .warnings
                .push("--cookie with a file is not supported and was ignored".to_owned()),
            Opt::UserAgent => self.headers.push(KeyValue::new("User-Agent", value)),
            Opt::Referer => self.headers.push(KeyValue::new("Referer", value)),
            Opt::Url => self.urls.push(value.to_owned()),
            Opt::MaxTime => match value.parse::<f64>() {
                Ok(seconds) if seconds.is_finite() && seconds >= 0.0 => {
                    // Whole milliseconds; a timeout beyond u64 milliseconds saturates.
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "finite, non-negative and rounded"
                    )]
                    let ms = (seconds * 1000.0).round() as u64;
                    self.settings.timeout_ms = Some(ms);
                }
                _ => self.warnings.push(format!(
                    "--max-time {value} is not a number of seconds and was ignored"
                )),
            },
            Opt::Insecure => self.settings.verify_tls = Some(false),
            Opt::Location => self.settings.follow_redirects = Some(true),
            Opt::Head => self.head = true,
            Opt::Get => self.get = true,
            Opt::Quiet => {}
            Opt::Unsupported { .. } => self
                .warnings
                .push(format!("--{long} is not supported and was ignored")),
        }
        Ok(())
    }

    /// `Name: value`, `Name;` for an empty value; `Name:` removes a curl default header,
    /// which ReqFlux has none of; `@file` reads headers from a file.
    fn header(&mut self, value: &str) {
        if value.starts_with('@') {
            self.warnings
                .push("-H with a file is not supported and was ignored".to_owned());
        } else if let Some((name, rest)) = value.split_once(':') {
            let rest = rest.trim_start();
            if rest.is_empty() {
                self.warnings.push(format!(
                    "-H {name}: removes a curl default header and was ignored"
                ));
            } else {
                self.headers.push(KeyValue::new(name.trim(), rest));
            }
        } else if let Some(name) = value.strip_suffix(';') {
            self.headers.push(KeyValue::new(name.trim(), ""));
        } else {
            // Only the first word: the rest can be a credential, such as `Bearer <token>`.
            let first = value.split_whitespace().next().unwrap_or_default();
            self.warnings
                .push(format!("-H {first} … is not a header and was ignored"));
        }
    }

    fn finish(mut self) -> Result<Parsed, CurlError> {
        let mut urls = std::mem::take(&mut self.urls).into_iter();
        let url = urls.next().ok_or(CurlError::NoUrl)?;
        for extra in urls {
            self.warnings.push(format!("extra URL {extra} was ignored"));
        }
        let url = if url.contains("://") || url.starts_with("{{") {
            url
        } else {
            format!("http://{url}")
        };
        let (url, mut params) = query::split(&url);

        if self.json {
            for (name, value) in [
                ("Content-Type", "application/json"),
                ("Accept", "application/json"),
            ] {
                if !has_header(&self.headers, name) {
                    self.headers.push(KeyValue::new(name, value));
                }
            }
        }

        let has_payload = !self.data.is_empty() || !self.form.is_empty();
        let body = if self.get {
            params.extend(body::query(
                std::mem::take(&mut self.data),
                &mut self.warnings,
            ));
            Body::None
        } else if !self.form.is_empty() {
            if !self.data.is_empty() {
                self.warnings.push(
                    "data together with -F is not supported; the data was ignored".to_owned(),
                );
            }
            Body::FormData {
                parts: std::mem::take(&mut self.form),
            }
        } else {
            let content_type = header_value(&self.headers, "Content-Type").map(str::to_owned);
            let (body, add_type) = body::body(
                std::mem::take(&mut self.data),
                content_type.as_deref(),
                &mut self.warnings,
            );
            if let Some(content_type) = add_type {
                self.headers
                    .push(KeyValue::new("Content-Type", content_type));
            }
            body
        };

        let method = match self.method {
            Some(verb) => Method::new(&verb)?,
            None if self.head => Method::HEAD,
            None if self.get => Method::GET,
            None if has_payload => Method::POST,
            None => Method::GET,
        };
        Ok(Parsed {
            method,
            url,
            params,
            headers: self.headers,
            auth: self.auth.unwrap_or_default(),
            body,
            settings: self.settings,
            warnings: self.warnings,
        })
    }
}

fn has_header(headers: &[KeyValue], name: &str) -> bool {
    header_value(headers, name).is_some()
}

fn header_value<'a>(headers: &'a [KeyValue], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.key.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}
