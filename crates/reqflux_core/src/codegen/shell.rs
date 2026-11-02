//! curl and HTTPie commands for a POSIX shell. Every value is in single quotes, so nothing
//! in it expands; a `'` is written `'\''`.

use super::{Parts, seconds, sent};
use crate::model::{Auth, Body, FormPart, Method, Request};

/// `text` in single quotes.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The comment lines, the first line, then one continued line per argument.
fn command(notes: &[String], first: String, args: Vec<String>) -> String {
    let mut lines: Vec<String> = notes.iter().map(|note| format!("# {note}")).collect();
    lines.push(
        std::iter::once(first)
            .chain(args)
            .collect::<Vec<_>>()
            .join(" \\\n  "),
    );
    lines.join("\n")
}

/// A `-F` text value that curl would read as a file (`@`, `<`), a quoted value (`"`) or
/// parameters (`;`) goes through `--form-string`.
fn form_text(key: &str, value: &str) -> String {
    let literal = value.starts_with(['@', '<', '"']) || value.contains(';');
    let flag = if literal { "--form-string" } else { "-F" };
    format!("{flag} {}", quote(&format!("{key}={value}")))
}

/// A `-F` file path, quoted with `"` when it holds a `;`, `"` or `\`.
fn form_file(key: &str, path: &str) -> String {
    let path = if path.contains([';', '"', '\\']) {
        format!("\"{}\"", path.replace('\\', r"\\").replace('"', "\\\""))
    } else {
        path.to_owned()
    };
    format!("-F {}", quote(&format!("{key}=@{path}")))
}

pub(super) fn curl(request: &Request, parts: &Parts) -> String {
    let has_body = !request.body.is_none();
    let method = &request.method;
    let mut first = "curl".to_owned();
    if *method == Method::HEAD {
        first.push_str(" -I");
    } else if !(*method == Method::GET && !has_body || *method == Method::POST && has_body) {
        first.push_str(&format!(" -X {method}"));
    }
    first.push(' ');
    first.push_str(&quote(&parts.url));

    let mut args: Vec<String> = parts
        .headers
        .iter()
        .map(|(name, value)| header(name, value, ": "))
        .collect();
    match &request.auth {
        Auth::Basic { username, password } => {
            args.push(format!("-u {}", quote(&format!("{username}:{password}"))));
        }
        Auth::Bearer { token } => args.push(format!("--oauth2-bearer {}", quote(token))),
        _ => args.extend(
            parts
                .auth_header
                .iter()
                .map(|(name, value)| header(name, value, ": ")),
        ),
    }
    match &request.body {
        Body::None => {}
        Body::Raw { .. } | Body::Graphql { .. } => {
            args.extend(
                parts
                    .text
                    .iter()
                    .map(|text| format!("--data-raw {}", quote(text))),
            );
        }
        Body::Urlencoded { fields } => {
            args.extend(sent(fields).map(|(key, value)| {
                format!("--data-urlencode {}", quote(&format!("{key}={value}")))
            }))
        }
        Body::FormData { parts } => args.extend(parts.iter().filter_map(|part| match part {
            FormPart::Text {
                key,
                value,
                enabled: true,
                ..
            } => Some(form_text(key, value)),
            FormPart::File {
                key,
                path,
                enabled: true,
                ..
            } => Some(form_file(key, path)),
            FormPart::Text { .. } | FormPart::File { .. } => None,
        })),
        Body::Binary { path } => args.push(format!("--data-binary {}", quote(&format!("@{path}")))),
    }
    let settings = &request.settings;
    if settings.verify_tls == Some(false) {
        args.push("-k".to_owned());
    }
    if settings.follow_redirects == Some(true) {
        args.push("-L".to_owned());
    }
    if let Some(ms) = settings.timeout_ms {
        args.push(format!("--max-time {}", seconds(ms)));
    }
    command(&parts.notes, first, args)
}

/// `-H 'Name: value'` for curl (`separator` `": "`) or `'Name:value'` for HTTPie (`":"`);
/// an empty value is `Name;` in both.
fn header(name: &str, value: &str, separator: &str) -> String {
    let item = if value.is_empty() {
        format!("{name};")
    } else {
        format!("{name}{separator}{value}")
    };
    if separator == ": " {
        format!("-H {}", quote(&item))
    } else {
        quote(&item)
    }
}

/// An HTTPie item key: `\` escapes the separators `:`, `=`, `@` and `;` and itself.
fn item_key(key: &str) -> String {
    let mut escaped = String::with_capacity(key.len());
    for c in key.chars() {
        if matches!(c, ':' | '=' | '@' | ';' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

pub(super) fn httpie(request: &Request, parts: &Parts) -> String {
    let mut first = vec!["http".to_owned()];
    match &request.body {
        Body::Urlencoded { .. } => first.push("--form".to_owned()),
        Body::FormData { .. } => first.push("--multipart".to_owned()),
        _ => {}
    }
    let settings = &request.settings;
    if settings.follow_redirects == Some(true) {
        first.push("--follow".to_owned());
    }
    if settings.verify_tls == Some(false) {
        first.push("--verify=no".to_owned());
    }
    if let Some(ms) = settings.timeout_ms {
        first.push(format!("--timeout={}", seconds(ms)));
    }
    if let Auth::Basic { username, password } = &request.auth {
        first.push(format!(
            "--auth {}",
            quote(&format!("{username}:{password}"))
        ));
    }
    first.push(request.method.to_string());
    first.push(quote(&parts.url));

    let mut args: Vec<String> = parts
        .headers
        .iter()
        .chain(parts.auth_header.iter())
        .map(|(name, value)| header(&item_key(name), value, ":"))
        .collect();
    match &request.body {
        Body::None => {}
        Body::Raw { .. } | Body::Graphql { .. } => {
            args.extend(
                parts
                    .text
                    .iter()
                    .map(|text| format!("--raw {}", quote(text))),
            );
        }
        Body::Urlencoded { fields } => args
            .extend(sent(fields).map(|(key, value)| quote(&format!("{}={value}", item_key(key))))),
        Body::FormData { parts } => args.extend(parts.iter().filter_map(|part| match part {
            FormPart::Text {
                key,
                value,
                enabled: true,
                ..
            } => Some(quote(&format!("{}={value}", item_key(key)))),
            FormPart::File {
                key,
                path,
                enabled: true,
                ..
            } => Some(quote(&format!("{}@{path}", item_key(key)))),
            FormPart::Text { .. } | FormPart::File { .. } => None,
        })),
        Body::Binary { path } => {
            let line = first.join(" ");
            let mut command_line = command(&parts.notes, line, args);
            command_line.push_str(&format!(" < {}", quote(path)));
            return command_line;
        }
    }
    command(&parts.notes, first.join(" "), args)
}
