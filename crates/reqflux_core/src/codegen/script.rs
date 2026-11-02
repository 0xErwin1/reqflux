//! Python `requests` and JavaScript `fetch`. Strings are JSON string literals, valid in
//! both languages.

use super::{Parts, json_string as q, sent};
use crate::model::{Auth, Body, FormPart, Method, Request};

pub(super) fn python(request: &Request, parts: &Parts) -> String {
    let mut out: String = parts
        .notes
        .iter()
        .map(|note| format!("# {note}\n"))
        .collect();
    out.push_str("import requests\n\nresponse = requests.request(\n");
    let mut arg = |line: String| {
        out.push_str("    ");
        out.push_str(&line);
        out.push_str(",\n");
    };
    arg(q(request.method.as_str()));
    arg(q(&parts.url));
    let headers: Vec<_> = parts
        .headers
        .iter()
        .chain(parts.auth_header.iter())
        .collect();
    if !headers.is_empty() {
        let entries: String = headers
            .iter()
            .map(|(name, value)| format!("        {}: {},\n", q(name), q(value)))
            .collect();
        arg(format!("headers={{\n{entries}    }}"));
    }
    if let Auth::Basic { username, password } = &request.auth {
        arg(format!("auth=({}, {})", q(username), q(password)));
    }
    match &request.body {
        Body::None => {}
        Body::Raw { .. } | Body::Graphql { .. } => {
            if let Some(text) = &parts.text {
                arg(format!("data={}.encode()", q(text)));
            }
        }
        Body::Urlencoded { fields } => {
            let entries: String = sent(fields)
                .map(|(key, value)| format!("        ({}, {}),\n", q(key), q(value)))
                .collect();
            arg(format!("data=[\n{entries}    ]"));
        }
        Body::FormData { parts } => {
            let entries: String = parts
                .iter()
                .filter_map(|part| match part {
                    FormPart::Text {
                        key,
                        value,
                        enabled: true,
                        ..
                    } => Some(format!("        ({}, (None, {})),\n", q(key), q(value))),
                    FormPart::File {
                        key,
                        path,
                        enabled: true,
                        ..
                    } => Some(format!(
                        "        ({}, open({}, \"rb\")),\n",
                        q(key),
                        q(path)
                    )),
                    FormPart::Text { .. } | FormPart::File { .. } => None,
                })
                .collect();
            arg(format!("files=[\n{entries}    ]"));
        }
        Body::Binary { path } => arg(format!("data=open({}, \"rb\")", q(path))),
    }
    let settings = &request.settings;
    if settings.verify_tls == Some(false) {
        arg("verify=False".to_owned());
    }
    match settings.follow_redirects {
        Some(true) => arg("allow_redirects=True".to_owned()),
        Some(false) => arg("allow_redirects=False".to_owned()),
        None => {}
    }
    if let Some(ms) = settings.timeout_ms {
        arg(format!("timeout={}", super::seconds(ms)));
    }
    out.push_str(")\nprint(response.text)\n");
    out
}

pub(super) fn fetch(request: &Request, parts: &Parts) -> String {
    let mut notes = parts.notes.clone();
    if request.settings.verify_tls == Some(false) {
        notes.push("fetch cannot turn off TLS verification (verify_tls = false)".to_owned());
    }
    let mut prelude = String::new();
    let mut options = Vec::new();
    if request.method != Method::GET {
        options.push(format!("method: {}", q(request.method.as_str())));
    }
    let mut headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .chain(parts.auth_header.iter())
        .map(|(name, value)| (q(name), q(value)))
        .collect();
    if let Auth::Basic { username, password } = &request.auth {
        headers.push((
            q("Authorization"),
            format!(
                "\"Basic \" + btoa({})",
                q(&format!("{username}:{password}"))
            ),
        ));
    }
    if !headers.is_empty() {
        let entries: String = headers
            .iter()
            .map(|(name, value)| format!("    {name}: {value},\n"))
            .collect();
        options.push(format!("headers: {{\n{entries}  }}"));
    }
    let blob = "import { openAsBlob } from \"node:fs\";\n\n";
    match &request.body {
        Body::None => {}
        Body::Raw { .. } | Body::Graphql { .. } => {
            options.extend(parts.text.iter().map(|text| format!("body: {}", q(text))));
        }
        Body::Urlencoded { fields } => {
            let entries: String = sent(fields)
                .map(|(key, value)| format!("    [{}, {}],\n", q(key), q(value)))
                .collect();
            options.push(format!("body: new URLSearchParams([\n{entries}  ])"));
        }
        Body::FormData { parts } => {
            let mut appends = String::from("const body = new FormData();\n");
            let mut files = false;
            for part in parts {
                match part {
                    FormPart::Text {
                        key,
                        value,
                        enabled: true,
                        ..
                    } => appends.push_str(&format!("body.append({}, {});\n", q(key), q(value))),
                    FormPart::File {
                        key,
                        path,
                        enabled: true,
                        ..
                    } => {
                        files = true;
                        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                        appends.push_str(&format!(
                            "body.append({}, await openAsBlob({}), {});\n",
                            q(key),
                            q(path),
                            q(name)
                        ));
                    }
                    FormPart::Text { .. } | FormPart::File { .. } => {}
                }
            }
            if files {
                prelude.push_str(blob);
            }
            prelude.push_str(&appends);
            prelude.push('\n');
            options.push("body".to_owned());
        }
        Body::Binary { path } => {
            prelude.push_str(blob);
            options.push(format!("body: await openAsBlob({})", q(path)));
        }
    }
    match request.settings.follow_redirects {
        Some(true) => options.push("redirect: \"follow\"".to_owned()),
        Some(false) => options.push("redirect: \"manual\"".to_owned()),
        None => {}
    }
    if let Some(ms) = request.settings.timeout_ms {
        options.push(format!("signal: AbortSignal.timeout({ms})"));
    }

    let mut out: String = notes.iter().map(|note| format!("// {note}\n")).collect();
    out.push_str(&prelude);
    out.push_str(&format!("const response = await fetch({}", q(&parts.url)));
    if !options.is_empty() {
        out.push_str(", {\n");
        for option in options {
            out.push_str(&format!("  {option},\n"));
        }
        out.push('}');
    }
    out.push_str(");\nconsole.log(await response.text());\n");
    out
}
