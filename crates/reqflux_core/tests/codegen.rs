//! `codegen::generate`: the request as written in curl, HTTPie, Python requests and
//! JavaScript fetch, variables left as `{{name}}` (v1-features S2).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

use pretty_assertions::assert_eq;
use reqflux_core::codegen::{self, Language};
use reqflux_core::model::{
    ApiKeyLocation, Auth, Body, FormPart, Id, KeyValue, Method, RawLanguage, Request, Settings,
};

fn json_post() -> Request {
    let mut request = Request::new(Id::new(), "R", Method::POST, "{{base}}/users");
    request.params = vec![KeyValue::new("q", "it's"), {
        let mut off = KeyValue::new("off", "1");
        off.enabled = false;
        off
    }];
    request.headers = vec![
        KeyValue::new("X-Note", "say \"hi\""),
        KeyValue::new("X-Empty", ""),
    ];
    request.auth = Auth::Basic {
        username: "ada".to_owned(),
        password: "{{password}}".to_owned(),
    };
    request.body = Body::Raw {
        language: RawLanguage::Json,
        text: "{\"name\":\"O'Hara\",\n\"n\":1}".to_owned(),
    };
    request.settings = Settings {
        timeout_ms: Some(2500),
        follow_redirects: Some(false),
        verify_tls: Some(false),
    };
    request
}

fn form_put() -> Request {
    let mut request = Request::new(Id::new(), "R", Method::PUT, "https://x.io/up");
    request.auth = Auth::Bearer {
        token: "{{token}}".to_owned(),
    };
    request.body = Body::FormData {
        parts: vec![
            FormPart::Text {
                key: "title".to_owned(),
                value: "a; b".to_owned(),
                description: None,
                enabled: true,
            },
            FormPart::File {
                key: "file".to_owned(),
                path: "docs/report.pdf".to_owned(),
                description: None,
                enabled: true,
            },
        ],
    };
    request.settings.follow_redirects = Some(true);
    request
}

fn urlencoded_get() -> Request {
    let mut request = Request::new(Id::new(), "R", Method::GET, "https://x.io/s");
    request.auth = Auth::ApiKey {
        key: "X-Key".to_owned(),
        value: "{{key}}".to_owned(),
        location: ApiKeyLocation::Header,
    };
    request.body = Body::Urlencoded {
        fields: vec![KeyValue::new("a", "1 2"), KeyValue::new("b", "&")],
    };
    request
}

#[test]
fn curl() {
    assert_eq!(
        codegen::generate(&json_post(), Language::Curl),
        r#"curl '{{base}}/users?q=it'\''s' \
  -H 'X-Note: say "hi"' \
  -H 'X-Empty;' \
  -H 'Content-Type: application/json' \
  -u 'ada:{{password}}' \
  --data-raw '{"name":"O'\''Hara",
"n":1}' \
  -k \
  --max-time 2.5"#
    );
    assert_eq!(
        codegen::generate(&form_put(), Language::Curl),
        r"curl -X PUT 'https://x.io/up' \
  --oauth2-bearer '{{token}}' \
  --form-string 'title=a; b' \
  -F 'file=@docs/report.pdf' \
  -L"
    );
    assert_eq!(
        codegen::generate(&urlencoded_get(), Language::Curl),
        r"curl -X GET 'https://x.io/s' \
  -H 'X-Key: {{key}}' \
  --data-urlencode 'a=1 2' \
  --data-urlencode 'b=&'"
    );
}

#[test]
fn httpie() {
    assert_eq!(
        codegen::generate(&json_post(), Language::Httpie),
        r#"http --verify=no --timeout=2.5 --auth 'ada:{{password}}' POST '{{base}}/users?q=it'\''s' \
  'X-Note:say "hi"' \
  'X-Empty;' \
  'Content-Type:application/json' \
  --raw '{"name":"O'\''Hara",
"n":1}'"#
    );
    assert_eq!(
        codegen::generate(&form_put(), Language::Httpie),
        r"http --multipart --follow PUT 'https://x.io/up' \
  'Authorization:Bearer {{token}}' \
  'title=a; b' \
  'file@docs/report.pdf'"
    );
    assert_eq!(
        codegen::generate(&urlencoded_get(), Language::Httpie),
        r"http --form GET 'https://x.io/s' \
  'X-Key:{{key}}' \
  'a=1 2' \
  'b=&'"
    );
}

#[test]
fn python() {
    assert_eq!(
        codegen::generate(&json_post(), Language::Python),
        r#"import requests

response = requests.request(
    "POST",
    "{{base}}/users?q=it's",
    headers={
        "X-Note": "say \"hi\"",
        "X-Empty": "",
        "Content-Type": "application/json",
    },
    auth=("ada", "{{password}}"),
    data="{\"name\":\"O'Hara\",\n\"n\":1}".encode(),
    verify=False,
    allow_redirects=False,
    timeout=2.5,
)
print(response.text)
"#
    );
    assert_eq!(
        codegen::generate(&form_put(), Language::Python),
        r#"import requests

response = requests.request(
    "PUT",
    "https://x.io/up",
    headers={
        "Authorization": "Bearer {{token}}",
    },
    files=[
        ("title", (None, "a; b")),
        ("file", open("docs/report.pdf", "rb")),
    ],
    allow_redirects=True,
)
print(response.text)
"#
    );
    assert_eq!(
        codegen::generate(&urlencoded_get(), Language::Python),
        r#"import requests

response = requests.request(
    "GET",
    "https://x.io/s",
    headers={
        "X-Key": "{{key}}",
    },
    data=[
        ("a", "1 2"),
        ("b", "&"),
    ],
)
print(response.text)
"#
    );
}

#[test]
fn fetch() {
    assert_eq!(
        codegen::generate(&json_post(), Language::Fetch),
        r#"// fetch cannot turn off TLS verification (verify_tls = false)
const response = await fetch("{{base}}/users?q=it's", {
  method: "POST",
  headers: {
    "X-Note": "say \"hi\"",
    "X-Empty": "",
    "Content-Type": "application/json",
    "Authorization": "Basic " + btoa("ada:{{password}}"),
  },
  body: "{\"name\":\"O'Hara\",\n\"n\":1}",
  redirect: "manual",
  signal: AbortSignal.timeout(2500),
});
console.log(await response.text());
"#
    );
    assert_eq!(
        codegen::generate(&form_put(), Language::Fetch),
        r#"import { openAsBlob } from "node:fs";

const body = new FormData();
body.append("title", "a; b");
body.append("file", await openAsBlob("docs/report.pdf"), "report.pdf");

const response = await fetch("https://x.io/up", {
  method: "PUT",
  headers: {
    "Authorization": "Bearer {{token}}",
  },
  body,
  redirect: "follow",
});
console.log(await response.text());
"#
    );
    assert_eq!(
        codegen::generate(&urlencoded_get(), Language::Fetch),
        r#"const response = await fetch("https://x.io/s", {
  headers: {
    "X-Key": "{{key}}",
  },
  body: new URLSearchParams([
    ["a", "1 2"],
    ["b", "&"],
  ]),
});
console.log(await response.text());
"#
    );
}

#[test]
fn graphql_binary_and_unsupported_auth() {
    let mut request = Request::new(Id::new(), "R", Method::POST, "https://x.io/graphql");
    request.body = Body::Graphql {
        query: "{ me { id } }".to_owned(),
        variables: String::new(),
    };
    assert_eq!(
        codegen::generate(&request, Language::Curl),
        r#"curl 'https://x.io/graphql' \
  -H 'Content-Type: application/json' \
  --data-raw '{"query":"{ me { id } }","variables":null}'"#
    );
    request.body = Body::Binary {
        path: "a b.bin".to_owned(),
    };
    request.auth = Auth::Other {
        kind: "digest".to_owned(),
        fields: toml::Table::new(),
    };
    assert_eq!(
        codegen::generate(&request, Language::Curl),
        "# auth digest is not generated\ncurl 'https://x.io/graphql' \\\n  --data-binary '@a b.bin'"
    );
    assert_eq!(
        codegen::generate(&request, Language::Httpie),
        "# auth digest is not generated\nhttp POST 'https://x.io/graphql' < 'a b.bin'"
    );
}

#[test]
fn languages_parse_from_their_names() {
    for language in Language::ALL {
        assert_eq!(language.as_str().parse::<Language>(), Ok(language));
    }
    assert_eq!(
        Language::ALL.map(Language::as_str),
        ["curl", "httpie", "python", "fetch"]
    );
    assert!("ruby".parse::<Language>().is_err());
}
