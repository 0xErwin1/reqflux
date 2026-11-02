//! `curl::parse`: commands copied from Chrome, Firefox and Postman, quoting, flags, warnings
//! and errors, and the round trip through `codegen` (v1-features S2).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use pretty_assertions::assert_eq;
use reqflux_core::codegen::{self, Language};
use reqflux_core::curl::{self, CurlError, Parsed};
use reqflux_core::model::{
    Auth, Body, FormPart, Id, KeyValue, Method, RawLanguage, Request, Settings,
};

fn parse(command: &str) -> Parsed {
    match curl::parse(command) {
        Ok(parsed) => parsed,
        Err(error) => panic!("{command}: {error}"),
    }
}

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

fn request(parsed: Parsed) -> Request {
    let mut request = Request::new(Id::new(), "R", Method::GET, "");
    parsed.apply(&mut request);
    request
}

const CHROME_BASH: &str = r#"curl 'https://api.example.com/v1/users?page=2&sort=name' \
  -H 'accept: application/json, text/plain, */*' \
  -H 'accept-language: en-US,en;q=0.9' \
  -H 'authorization: Bearer eyJhbGciOi' \
  -H 'content-type: application/json' \
  -H 'sec-ch-ua: "Chromium";v="120", "Not?A_Brand";v="24"' \
  -H 'user-agent: Mozilla/5.0 (X11; Linux x86_64)' \
  --data-raw '{"name":"Ada","tags":["a","b"]}' \
  --compressed"#;

const CHROME_ANSI_C: &str = r#"curl 'https://api.example.com/notes' \
  -H 'content-type: application/json' \
  --data-raw $'{"note":"it\'s\\nfine \u00e9"}' \
  --compressed"#;

const CHROME_CMD: &str = "curl ^\"https://api.example.com/v1/users?page=2^\" ^\r\n  -H ^\"accept: application/json^\" ^\r\n  -H ^\"content-type: application/json^\" ^\r\n  --data-raw ^\"^{^\\^\"name^\\^\":^\\^\"Ada 100^%^\\^\"^}^\" ^\r\n  --compressed";

const FIREFOX: &str = "curl 'https://api.example.com/login' -X POST -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0' -H 'Accept: */*' -H 'Content-Type: application/x-www-form-urlencoded' -H 'Origin: https://app.example.com' -H 'Connection: keep-alive' -H 'Cookie: session=abc; theme=dark' --data-raw 'user=ada%40example.com&pass=s3cr%26t+x'";

const POSTMAN_FORM: &str = r#"curl --location 'https://api.example.com/upload?v=1' \
--header 'Authorization: Basic YWRhOnB3' \
--form 'file=@"/home/ada/report.pdf"' \
--form 'name="quarterly; final"'"#;

const POSTMAN_URLENCODED: &str = r"curl --location --request PATCH '{{base}}/items/{{id}}' \
--header 'Content-Type: application/x-www-form-urlencoded' \
--data-urlencode 'title=Hello world' \
--data-urlencode 'note=a&b=c'";

const POSTMAN_JSON: &str = r#"curl --location 'https://api.example.com/items' \
--header 'Content-Type: application/json' \
--data '{
    "title": "Hello",
    "done": false
}'"#;

#[test]
fn chrome_bash_copy_keeps_url_query_headers_and_json_body() {
    let parsed = parse(CHROME_BASH);
    assert_eq!(parsed.method, Method::POST);
    assert_eq!(parsed.url, "https://api.example.com/v1/users");
    assert_eq!(parsed.params, [kv("page", "2"), kv("sort", "name")]);
    assert_eq!(
        parsed.headers,
        [
            kv("accept", "application/json, text/plain, */*"),
            kv("accept-language", "en-US,en;q=0.9"),
            kv("authorization", "Bearer eyJhbGciOi"),
            kv("content-type", "application/json"),
            kv("sec-ch-ua", r#""Chromium";v="120", "Not?A_Brand";v="24""#),
            kv("user-agent", "Mozilla/5.0 (X11; Linux x86_64)"),
        ]
    );
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: r#"{"name":"Ada","tags":["a","b"]}"#.to_owned(),
        }
    );
    assert_eq!(parsed.auth, Auth::Inherit);
    assert_eq!(parsed.settings, Settings::default());
    assert_eq!(parsed.warnings, Vec::<String>::new());
}

#[test]
fn chrome_ansi_c_quoting_decodes_escapes() {
    let parsed = parse(CHROME_ANSI_C);
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: "{\"note\":\"it's\\nfine \u{e9}\"}".to_owned(),
        }
    );
}

#[test]
fn chrome_cmd_copy_decodes_carets_and_escaped_quotes() {
    let parsed = parse(CHROME_CMD);
    assert_eq!(parsed.method, Method::POST);
    assert_eq!(parsed.url, "https://api.example.com/v1/users");
    assert_eq!(parsed.params, [kv("page", "2")]);
    assert_eq!(
        parsed.headers,
        [
            kv("accept", "application/json"),
            kv("content-type", "application/json")
        ]
    );
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: r#"{"name":"Ada 100%"}"#.to_owned(),
        }
    );
    assert_eq!(parsed.warnings, Vec::<String>::new());
}

#[test]
fn firefox_form_post_becomes_urlencoded_fields_decoded() {
    let parsed = parse(FIREFOX);
    assert_eq!(parsed.method, Method::POST);
    assert_eq!(
        parsed.body,
        Body::Urlencoded {
            fields: vec![kv("user", "ada@example.com"), kv("pass", "s3cr&t x")],
        }
    );
    assert_eq!(parsed.headers.len(), 6);
    assert_eq!(parsed.headers[5], kv("Cookie", "session=abc; theme=dark"));
}

#[test]
fn postman_form_export_strips_quotes_and_keeps_files() {
    let parsed = parse(POSTMAN_FORM);
    assert_eq!(parsed.method, Method::POST);
    assert_eq!(parsed.settings.follow_redirects, Some(true));
    assert_eq!(parsed.params, [kv("v", "1")]);
    assert_eq!(
        parsed.body,
        Body::FormData {
            parts: vec![
                FormPart::File {
                    key: "file".to_owned(),
                    path: "/home/ada/report.pdf".to_owned(),
                    description: None,
                    enabled: true,
                },
                FormPart::Text {
                    key: "name".to_owned(),
                    value: "quarterly; final".to_owned(),
                    description: None,
                    enabled: true,
                },
            ],
        }
    );
}

#[test]
fn postman_urlencoded_export_keeps_variables_and_method() {
    let parsed = parse(POSTMAN_URLENCODED);
    assert_eq!(parsed.method, Method::PATCH);
    assert_eq!(parsed.url, "{{base}}/items/{{id}}");
    assert_eq!(
        parsed.body,
        Body::Urlencoded {
            fields: vec![kv("title", "Hello world"), kv("note", "a&b=c")],
        }
    );
}

#[test]
fn postman_multiline_json_body() {
    let parsed = parse(POSTMAN_JSON);
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: "{\n    \"title\": \"Hello\",\n    \"done\": false\n}".to_owned(),
        }
    );
}

#[test]
fn basic_auth_insecure_redirects_and_timeout() {
    let parsed = parse("curl -kL -u 'ada:{{password}}' -m 2.5 http://localhost:8080/x");
    assert_eq!(parsed.method, Method::GET);
    assert_eq!(
        parsed.auth,
        Auth::Basic {
            username: "ada".to_owned(),
            password: "{{password}}".to_owned(),
        }
    );
    assert_eq!(
        parsed.settings,
        Settings {
            timeout_ms: Some(2500),
            follow_redirects: Some(true),
            verify_tls: Some(false),
        }
    );
}

#[test]
fn methods_attached_values_and_head() {
    assert_eq!(parse("curl -XDELETE https://x.io").method, Method::DELETE);
    assert_eq!(parse("curl -X put https://x.io").method, Method::PUT);
    assert_eq!(parse("curl -I https://x.io").method, Method::HEAD);
    let parsed = parse("curl -HAccept:text/html --url https://x.io -d a=1");
    assert_eq!(parsed.headers, [kv("Accept", "text/html")]);
    assert_eq!(parsed.method, Method::POST);
}

#[test]
fn get_flag_moves_data_to_the_query() {
    let parsed = parse("curl -G https://x.io/s?q=1 -d 'a=1' --data-urlencode 'b=x y'");
    assert_eq!(parsed.method, Method::GET);
    assert_eq!(
        parsed.params,
        [kv("q", "1"), kv("a", "1"), kv("b", "x%20y")]
    );
    assert_eq!(parsed.body, Body::None);
}

#[test]
fn data_without_content_type_and_files() {
    let parsed = parse("curl https://x.io -d 'plain text'");
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Text,
            text: "plain text".to_owned(),
        }
    );
    assert_eq!(
        parsed.headers,
        [kv("Content-Type", "application/x-www-form-urlencoded")]
    );
    let parsed = parse("curl https://x.io --data-binary @payload.bin");
    assert_eq!(
        parsed.body,
        Body::Binary {
            path: "payload.bin".to_owned()
        }
    );
    let parsed = parse("curl https://x.io -d a=1 -d b=2");
    assert_eq!(
        parsed.body,
        Body::Urlencoded {
            fields: vec![kv("a", "1"), kv("b", "2")]
        }
    );
}

#[test]
fn json_flag_and_user_agent_cookie_referer() {
    let parsed = parse(r#"curl --json '{"a":1}' -A 'ua/1' -b 'k=v' -e https://r.io https://x.io"#);
    assert_eq!(parsed.method, Method::POST);
    assert_eq!(
        parsed.headers,
        [
            kv("User-Agent", "ua/1"),
            kv("Cookie", "k=v"),
            kv("Referer", "https://r.io"),
            kv("Content-Type", "application/json"),
            kv("Accept", "application/json"),
        ]
    );
    assert_eq!(
        parsed.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: r#"{"a":1}"#.to_owned()
        }
    );
}

#[test]
fn bearer_flag_and_bash_double_quotes() {
    let parsed =
        parse(r#"curl --oauth2-bearer "{{token}}" "https://x.io/a b" -H "X-Q: \"q\" \$HOME""#);
    assert_eq!(
        parsed.auth,
        Auth::Bearer {
            token: "{{token}}".to_owned()
        }
    );
    assert_eq!(parsed.url, "https://x.io/a b");
    assert_eq!(parsed.headers, [kv("X-Q", "\"q\" $HOME")]);
}

#[test]
fn url_without_scheme_gets_http() {
    assert_eq!(parse("curl example.com/a").url, "http://example.com/a");
    assert_eq!(parse("curl '{{base}}/a'").url, "{{base}}/a");
}

#[test]
fn unknown_and_unsupported_flags_are_warnings() {
    let parsed = parse(
        "curl -s --proxy http://p:3128 --frobnicate -o out.txt -F 'f=@a.png;type=image/png' https://x.io https://y.io",
    );
    assert_eq!(parsed.url, "https://x.io");
    assert_eq!(
        parsed.warnings,
        [
            "--proxy is not supported and was ignored",
            "unknown option --frobnicate was ignored",
            "--output is not supported and was ignored",
            "-F f: type=image/png is not supported and was ignored",
            "extra URL https://y.io was ignored",
        ]
    );
}

#[test]
fn warnings_never_echo_an_option_value_or_a_malformed_header() {
    let parsed = parse(
        "curl --oauth2-token=lit3ral-t0ken -H 'Authorization Bearer lit3ral-h3ader' https://x.io",
    );
    assert_eq!(
        parsed.warnings,
        [
            "unknown option --oauth2-token was ignored",
            "-H Authorization … is not a header and was ignored",
        ]
    );
    for warning in &parsed.warnings {
        assert!(!warning.contains("lit3ral"), "{warning}");
    }
}

#[test]
fn errors_name_the_problem() {
    assert_eq!(curl::parse("   "), Err(CurlError::Empty));
    assert_eq!(
        curl::parse("wget https://x.io"),
        Err(CurlError::NotCurl("wget".to_owned()))
    );
    assert_eq!(
        curl::parse("curl 'https://x.io"),
        Err(CurlError::Unterminated)
    );
    assert_eq!(
        curl::parse("curl -H"),
        Err(CurlError::MissingValue("-H".to_owned()))
    );
    assert_eq!(curl::parse("curl -s"), Err(CurlError::NoUrl));
    assert_eq!(
        CurlError::NotCurl("wget".to_owned()).to_string(),
        "not a curl command: starts with `wget`"
    );
}

#[test]
fn apply_replaces_the_request_but_keeps_identity() {
    let id = Id::new();
    let mut target = Request::new(id, "Mine", Method::PUT, "https://old.io");
    target.headers.push(kv("Old", "1"));
    let warnings = parse("curl -X POST https://new.io -H 'A: b'").apply(&mut target);
    assert!(warnings.is_empty());
    assert_eq!(target.id, id);
    assert_eq!(target.name, "Mine");
    assert_eq!(target.method, Method::POST);
    assert_eq!(target.url, "https://new.io");
    assert_eq!(target.headers, [kv("A", "b")]);
}

/// Every real command survives parse -> generate curl -> parse unchanged.
#[test]
fn real_commands_round_trip_through_curl_generation() {
    for command in [
        CHROME_BASH,
        CHROME_ANSI_C,
        CHROME_CMD,
        FIREFOX,
        POSTMAN_FORM,
        POSTMAN_URLENCODED,
        POSTMAN_JSON,
        "curl -kL -u 'ada:{{password}}' -m 2.5 http://localhost:8080/x",
        "curl -X DELETE 'https://x.io/it'\"'\"'s'",
        "curl https://x.io --data-binary @payload.bin",
        "curl --oauth2-bearer '{{token}}' https://x.io -F 'a=@x' --form-string 'b=@literal'",
    ] {
        let first = request(parse(command));
        let generated = codegen::generate(&first, Language::Curl);
        let mut second = request(parse(&generated));
        second.id = first.id;
        assert_eq!(second, first, "{command}\n--- generated ---\n{generated}");
    }
}
