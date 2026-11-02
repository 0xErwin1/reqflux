//! A pasted `curl` command as a request (v1-features S2).
//!
//! [`parse`] reads the commands browsers ("Copy as cURL" in Chrome, for bash or `cmd`, and
//! Firefox) and Postman write, and the common hand-written forms. Values stay as written:
//! `{{variables}}` and `$HOME` are not expanded. Every option that changes the request but
//! has no place in a ReqFlux request, and every unknown option, becomes a warning; options
//! that only change curl's own output (`-s`, `-v`, `-i`, `--compressed`, ...) are accepted
//! silently because ReqFlux shows the whole response and always decompresses it.

mod body;
mod lex;
mod options;

use crate::model::{Auth, Body, KeyValue, Method, MethodError, Request, Settings};

/// A request read from a curl command, and what of it was ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The method: `-X`, else `HEAD` with `-I`, `GET` with `-G`, `POST` with data or a form,
    /// else `GET`.
    pub method: Method,
    /// The URL without its query string; `http://` is added when it has no scheme.
    pub url: String,
    /// The query of the URL, and the data of `-G`.
    pub params: Vec<KeyValue>,
    /// The headers, in order, with those of `-A`, `-b`, `-e` and `--json`.
    pub headers: Vec<KeyValue>,
    /// `-u` as basic auth and `--oauth2-bearer` as a bearer token; otherwise inherit.
    pub auth: Auth,
    /// The body of `-d` and its variants, `--json` or `-F`.
    pub body: Body,
    /// `-m`, `-L` and `-k`.
    pub settings: Settings,
    /// One line per option or value that was ignored.
    pub warnings: Vec<String>,
}

/// Why a command is not a curl request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurlError {
    /// The command is blank.
    #[error("the curl command is empty")]
    Empty,
    /// The first word is not `curl`.
    #[error("not a curl command: starts with `{0}`")]
    NotCurl(String),
    /// A quote is not closed or a backslash ends the command.
    #[error("the curl command has an unterminated quote or escape")]
    Unterminated,
    /// An option that takes a value ends the command.
    #[error("the curl option {0} needs a value")]
    MissingValue(String),
    /// The command has no URL.
    #[error("the curl command has no URL")]
    NoUrl,
    /// `-X` names an invalid method.
    #[error("invalid curl method: {0}")]
    Method(#[from] MethodError),
}

/// Reads `command` as a curl invocation.
///
/// # Errors
///
/// A blank command, one that does not start with `curl`, an unterminated quote, an option
/// without its value, no URL or an invalid `-X` method.
pub fn parse(command: &str) -> Result<Parsed, CurlError> {
    let words = lex::words(command)?;
    let Some((program, args)) = words.split_first() else {
        return Err(CurlError::Empty);
    };
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    if !matches!(name, "curl" | "curl.exe") {
        return Err(CurlError::NotCurl(program.clone()));
    }
    options::read(args)
}

impl Parsed {
    /// Replaces the method, URL, params, headers, auth, body and settings of `request` with
    /// these, keeping its id, name and extensions, and returns the warnings.
    pub fn apply(self, request: &mut Request) -> Vec<String> {
        request.method = self.method;
        request.url = self.url;
        request.params = self.params;
        request.headers = self.headers;
        request.auth = self.auth;
        request.body = self.body;
        request.settings = self.settings;
        self.warnings
    }
}
