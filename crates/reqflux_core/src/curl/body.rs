//! The data and form options of a curl command, as a body or query params.

use super::options::DataKind;
use crate::model::{Body, FormPart, KeyValue, RawLanguage};

const FORM_TYPE: &str = "application/x-www-form-urlencoded";

/// One `-d`, `--data-*` or `--json` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Data {
    /// Sent as written.
    Text(String),
    /// `--data-urlencode name=content`: `content` is encoded on the wire.
    Field(String, String),
    /// `@file`: the bytes of a file.
    File(String),
}

/// The data of one option.
pub(super) fn data(kind: DataKind, value: &str) -> Data {
    match kind {
        DataKind::Data => match value.strip_prefix('@') {
            Some(path) => Data::File(path.to_owned()),
            None => Data::Text(value.to_owned()),
        },
        DataKind::Raw => Data::Text(value.to_owned()),
        DataKind::Urlencode => match value.split_once('=') {
            Some(("", content)) => Data::Text(encode(content)),
            Some((name, content)) => Data::Field(name.to_owned(), content.to_owned()),
            None => Data::Text(encode(value)),
        },
    }
}

/// The body of `data`, and the `Content-Type` to add: curl sends a form type when no header
/// sets one, which only plain text that is neither JSON nor `key=value` pairs needs written.
pub(super) fn body(
    mut data: Vec<Data>,
    content_type: Option<&str>,
    warnings: &mut Vec<String>,
) -> (Body, Option<&'static str>) {
    if data.is_empty() {
        return (Body::None, None);
    }
    if let [Data::File(path)] = data.as_slice() {
        return (Body::Binary { path: path.clone() }, None);
    }
    if data.iter().any(|part| matches!(part, Data::File(_))) {
        warnings.push(
            "@file together with other data is not supported; the file was ignored".to_owned(),
        );
        data.retain(|part| !matches!(part, Data::File(_)));
    }
    if let Some(content_type) = content_type {
        if content_type.to_ascii_lowercase().starts_with(FORM_TYPE) {
            return (
                Body::Urlencoded {
                    fields: fields(&data),
                },
                None,
            );
        }
        let body = Body::Raw {
            language: language(content_type),
            text: joined(&data),
        };
        return (body, None);
    }
    if data.iter().all(|part| matches!(part, Data::Field(..))) {
        return (
            Body::Urlencoded {
                fields: fields(&data),
            },
            None,
        );
    }
    let text = joined(&data);
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let body = Body::Raw {
            language: RawLanguage::Json,
            text,
        };
        return (body, None);
    }
    if form_shaped(&text) {
        return (
            Body::Urlencoded {
                fields: fields(&data),
            },
            None,
        );
    }
    let body = Body::Raw {
        language: RawLanguage::Text,
        text,
    };
    (body, Some(FORM_TYPE))
}

/// The data of `-G` as query params, as written; `--data-urlencode` content is encoded.
pub(super) fn query(data: Vec<Data>, warnings: &mut Vec<String>) -> Vec<KeyValue> {
    let mut params = Vec::new();
    for part in data {
        match part {
            Data::Field(key, value) => params.push(KeyValue::new(key, encode(&value))),
            Data::Text(text) => {
                params.extend(text.split('&').filter(|pair| !pair.is_empty()).map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    KeyValue::new(key, value)
                }))
            }
            Data::File(path) => {
                warnings.push(format!("-G with @{path} is not supported and was ignored"))
            }
        }
    }
    params
}

/// One `-F` (or, with `literal`, `--form-string`) value as a part, and its warnings.
pub(super) fn form(value: &str, literal: bool) -> (Option<FormPart>, Vec<String>) {
    let Some((key, value)) = value.split_once('=') else {
        return (
            None,
            vec![format!("-F {value} is not a form field and was ignored")],
        );
    };
    let text = |value: String| FormPart::Text {
        key: key.to_owned(),
        value,
        description: None,
        enabled: true,
    };
    let file = |path: String| FormPart::File {
        key: key.to_owned(),
        path,
        description: None,
        enabled: true,
    };
    if literal {
        return (Some(text(value.to_owned())), Vec::new());
    }
    let mut warnings = Vec::new();
    let (part, params) = if let Some(path) = value.strip_prefix('@') {
        let (path, params) = value_and_params(path);
        (file(path), params)
    } else if let Some(path) = value.strip_prefix('<') {
        warnings.push(format!(
            "-F {key}: <file is sent as a file upload, not as text"
        ));
        let (path, params) = value_and_params(path);
        (file(path), params)
    } else {
        let (value, params) = value_and_params(value);
        (text(value), params)
    };
    warnings.extend(
        params
            .split(';')
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .map(|param| format!("-F {key}: {param} is not supported and was ignored")),
    );
    (Some(part), warnings)
}

/// A `-F` value, unquoted when it is in `"` quotes (`\"` and `\\` escaped), and the
/// `;type=`, `;filename=`, `;headers=` or `;encoder=` parameters after it.
fn value_and_params(text: &str) -> (String, &str) {
    if let Some(quoted) = text.strip_prefix('"') {
        let mut value = String::new();
        let mut chars = quoted.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => return (value, quoted.get(at + 1..).unwrap_or_default()),
                '\\' => match chars.next() {
                    Some((_, escaped @ ('"' | '\\'))) => value.push(escaped),
                    Some((_, other)) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => value.push('\\'),
                },
                other => value.push(other),
            }
        }
        return (format!("\"{quoted}"), "");
    }
    let param = text.match_indices(';').map(|(at, _)| at).find(|&at| {
        let rest = text.get(at + 1..).unwrap_or_default().trim_start();
        ["type=", "filename=", "headers=", "encoder="]
            .iter()
            .any(|name| rest.starts_with(name))
    });
    match param {
        Some(at) => (
            text.get(..at).unwrap_or_default().to_owned(),
            text.get(at..).unwrap_or_default(),
        ),
        None => (text.to_owned(), ""),
    }
}

/// Every piece of `data` as urlencoded fields, decoded.
fn fields(data: &[Data]) -> Vec<KeyValue> {
    let mut fields = Vec::new();
    for part in data {
        match part {
            Data::Field(key, value) => fields.push(KeyValue::new(key.clone(), value.clone())),
            Data::Text(text) => {
                fields.extend(text.split('&').filter(|pair| !pair.is_empty()).map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    KeyValue::new(decode(key), decode(value))
                }))
            }
            Data::File(_) => {}
        }
    }
    fields
}

/// The pieces joined by `&` as curl sends them.
fn joined(data: &[Data]) -> String {
    data.iter()
        .filter_map(|part| match part {
            Data::Text(text) => Some(text.clone()),
            Data::Field(key, value) => Some(format!("{key}={}", encode(value))),
            Data::File(_) => None,
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Whether `text` reads as `key=value` pairs joined by `&`.
fn form_shaped(text: &str) -> bool {
    !text.is_empty()
        && !text.chars().any(char::is_whitespace)
        && text
            .split('&')
            .all(|pair| pair.split_once('=').is_some_and(|(key, _)| !key.is_empty()))
}

fn language(content_type: &str) -> RawLanguage {
    let content_type = content_type.to_ascii_lowercase();
    if content_type.contains("json") {
        RawLanguage::Json
    } else if content_type.contains("xml") {
        RawLanguage::Xml
    } else if content_type.contains("html") {
        RawLanguage::Html
    } else if content_type.contains("javascript") {
        RawLanguage::Javascript
    } else {
        RawLanguage::Text
    }
}

/// Percent-encodes every byte but the unreserved ones, as `--data-urlencode` does.
pub(crate) fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Decodes `%XX` and `+` of a form value; an invalid escape stays as written.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (byte, hex) {
            (b'%', Some(value)) => {
                decoded.push(value);
                at += 3;
            }
            (b'+', _) => {
                decoded.push(b' ');
                at += 1;
            }
            _ => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
