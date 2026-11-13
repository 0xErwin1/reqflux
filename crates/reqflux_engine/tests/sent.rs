//! The redacted copy of a prepared request that history records (history S3).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use common::{collection, options, post, prepare, prepare_with, request, single, workspace};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Auth, Body, KeyValue, RawLanguage, Variable};
use reqflux_engine::{MASK, SentBody, SentRequest};

fn secret(key: &str, value: &str) -> Variable {
    Variable {
        secret: true,
        ..Variable::new(key, value)
    }
}

fn header<'a>(sent: &'a SentRequest, name: &str) -> &'a str {
    match sent.headers.iter().find(|(key, _)| key == name) {
        Some((_, value)) => value,
        None => panic!("no header {name} in {:?}", sent.headers),
    }
}

fn raw(text: &str) -> Body {
    Body::Raw {
        language: RawLanguage::Text,
        text: text.to_owned(),
    }
}

#[test]
fn the_mask_is_eight_bullets() {
    assert_eq!(MASK, "••••••••");
}

#[test]
fn a_secret_variable_is_masked_in_the_header_query_and_body() {
    let mut doc = post("https://api.test/v1?static={{plain}}");
    doc.headers = vec![KeyValue::new("X-Key", "key {{token}}")];
    doc.params = vec![KeyValue::new("t", "{{token}}")];
    doc.body = raw("token={{token}}; plain={{plain}}");
    let ws = workspace(
        vec![secret("token", "s3cr3t"), Variable::new("plain", "visible")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert_eq!(header(&sent, "X-Key"), format!("key {MASK}"));
    assert_eq!(
        sent.url,
        format!("https://api.test/v1?static=visible&t={MASK}")
    );
    assert_eq!(
        sent.body,
        SentBody::Text(format!("token={MASK}; plain=visible"))
    );
}

#[test]
fn a_query_secret_with_reserved_characters_is_masked_once_encoded() {
    let mut doc = post("https://api.test/");
    doc.params = vec![KeyValue::new("t", "{{token}}")];
    let ws = workspace(
        vec![secret("token", "a b&c")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert!(!sent.url.contains("a%20b%26c"), "{}", sent.url);
    assert!(!sent.url.contains("a b&c"), "{}", sent.url);
}

#[test]
fn a_literal_bearer_token_is_masked() {
    let mut doc = post("https://api.test/");
    doc.auth = Auth::Bearer {
        token: "literal-token".to_owned(),
    };
    let sent = prepare(&single(doc)).sent();

    assert_eq!(header(&sent, "Authorization"), format!("Bearer {MASK}"));
}

#[test]
fn a_literal_basic_password_is_masked() {
    let mut doc = post("https://api.test/");
    doc.auth = Auth::Basic {
        username: "user".to_owned(),
        password: "pass".to_owned(),
    };
    let sent = prepare(&single(doc)).sent();

    assert_eq!(header(&sent, "Authorization"), format!("Basic {MASK}"));
}

#[test]
fn a_literal_api_key_value_is_masked_but_not_its_name() {
    let mut doc = post("https://api.test/");
    doc.auth = Auth::ApiKey {
        key: "X-Api-Key".to_owned(),
        value: "k-123".to_owned(),
        location: reqflux_core::model::ApiKeyLocation::Header,
    };
    let sent = prepare(&single(doc)).sent();

    assert_eq!(header(&sent, "X-Api-Key"), MASK);
}

#[test]
fn a_non_secret_value_is_not_masked() {
    let mut doc = post("https://api.test/{{plain}}");
    doc.headers = vec![KeyValue::new("X-Plain", "{{plain}}")];
    let ws = workspace(
        vec![Variable::new("plain", "visible")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert_eq!(sent.url, "https://api.test/visible");
    assert_eq!(header(&sent, "X-Plain"), "visible");
}

#[test]
fn a_secret_inside_another_word_is_still_masked() {
    let mut doc = post("https://api.test/");
    doc.body = raw("prefix{{token}}suffix and abcxyz");
    let ws = workspace(
        vec![secret("token", "abc")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    // Text replacement masks every occurrence, also one the secret did not produce.
    assert_eq!(
        sent.body,
        SentBody::Text(format!("prefix{MASK}suffix and {MASK}xyz"))
    );
}

#[test]
fn a_local_override_of_a_secret_is_masked_with_its_resolved_value() {
    let mut doc = post("https://api.test/");
    doc.headers = vec![KeyValue::new("X-Key", "{{token}}")];
    let ws = workspace(
        vec![secret("token", "stored")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare_with(&ws, "c/r", &options(None, &[("token", "override")])).sent();

    assert_eq!(header(&sent, "X-Key"), MASK);
}

#[test]
fn form_bodies_are_text_and_multipart_and_files_are_markers() {
    let mut doc = post("https://api.test/");
    doc.body = Body::Urlencoded {
        fields: vec![KeyValue::new("a", "1 2"), KeyValue::new("b", "{{token}}")],
    };
    let ws = workspace(
        vec![secret("token", "s3cr3t")],
        vec![collection("c", vec![request("r", doc.clone())])],
    );
    assert_eq!(
        prepare(&ws).sent().body,
        SentBody::Text(format!("a=1+2&b={MASK}"))
    );

    doc.body = Body::Binary {
        path: "data.bin".to_owned(),
    };
    let sent = prepare(&single(doc)).sent();
    assert!(matches!(sent.body, SentBody::Omitted(_)), "{:?}", sent.body);
}

#[test]
fn a_secret_value_reached_without_its_name_is_still_masked() {
    // A pre-request script can copy a secret into another variable, or a value can repeat
    // a secret literally: the request never names the secret, but its value is sent.
    let mut doc = post("https://api.test/");
    doc.headers = vec![KeyValue::new("X-Copy", "{{copy}}")];
    let ws = workspace(
        vec![secret("token", "s3cr3t"), Variable::new("copy", "s3cr3t")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert_eq!(header(&sent, "X-Copy"), MASK);
}

#[test]
fn a_secret_with_json_specials_in_a_graphql_query_is_masked() {
    for (value, escaped) in [("pa\"ss", "pa\\\"ss"), ("pa\\ss", "pa\\\\ss")] {
        let mut doc = post("https://api.test/graphql");
        doc.body = Body::Graphql {
            query: "query { login(password: \"{{pw}}\") }".to_owned(),
            variables: String::new(),
        };
        let ws = workspace(
            vec![secret("pw", value)],
            vec![collection("c", vec![request("r", doc)])],
        );

        let SentBody::Text(body) = prepare(&ws).sent().body else {
            panic!("expected a text body");
        };

        assert!(body.contains(MASK), "{body}");
        assert!(!body.contains(escaped), "{value}: {body}");
        assert!(!body.contains(value), "{value}: {body}");
    }
}

#[test]
fn an_api_key_in_the_query_is_masked_encoded() {
    let mut doc = post("https://api.test/");
    doc.auth = Auth::ApiKey {
        key: "api_key".to_owned(),
        value: "a b+c/d€".to_owned(),
        location: reqflux_core::model::ApiKeyLocation::Query,
    };

    let sent = prepare(&single(doc)).sent();

    assert_eq!(sent.url, format!("https://api.test/?api_key={MASK}"));
}

#[test]
fn a_one_character_secret_does_not_change_header_names() {
    let mut doc = post("https://api.test/");
    doc.headers = vec![
        KeyValue::new("Content-Type", "text/plain"),
        KeyValue::new("X-Key", "{{token}}"),
    ];
    let ws = workspace(
        vec![secret("token", "e")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();
    let names: Vec<&str> = sent.headers.iter().map(|(key, _)| key.as_str()).collect();

    assert_eq!(names, ["Content-Type", "X-Key"]);
    assert_eq!(header(&sent, "X-Key"), MASK);
}

#[test]
fn authorization_proxy_authorization_and_cookie_values_are_always_masked() {
    let mut doc = post("https://api.test/");
    doc.headers = vec![
        KeyValue::new("Authorization", "Token typed-in-clear"),
        KeyValue::new("proxy-authorization", "Basic dTpw"),
        KeyValue::new("Cookie", "session=abc; theme=dark"),
        KeyValue::new("Authorization", "opaque"),
        KeyValue::new("X-Other", "visible"),
    ];

    let sent = prepare(&single(doc)).sent();

    assert_eq!(
        sent.headers,
        vec![
            ("Authorization".to_owned(), format!("Token {MASK}")),
            ("proxy-authorization".to_owned(), format!("Basic {MASK}")),
            ("Cookie".to_owned(), MASK.to_owned()),
            ("Authorization".to_owned(), MASK.to_owned()),
            ("X-Other".to_owned(), "visible".to_owned()),
        ]
    );
}

#[test]
fn every_scope_value_of_a_secret_is_masked_not_only_the_winner() {
    let mut doc = post("https://api.test/");
    doc.body = raw("{{token}} global-tok collection-tok");
    let mut coll = collection("c", vec![request("r", doc)]);
    coll.collection.variables = vec![secret("token", "collection-tok")];
    let ws = workspace(vec![secret("token", "global-tok")], vec![coll]);

    let sent = prepare_with(&ws, "c/r", &options(None, &[("token", "local-tok")])).sent();

    assert_eq!(sent.body, SentBody::Text(format!("{MASK} {MASK} {MASK}")));
}

#[test]
fn the_masker_masks_text_and_its_debug_hides_the_secrets() {
    let mut doc = post("https://api.test/");
    doc.params = vec![KeyValue::new("t", "{{token}}")];
    let ws = workspace(
        vec![secret("token", "a b&c")],
        vec![collection("c", vec![request("r", doc)])],
    );
    let masker = prepare(&ws).masker();

    assert_eq!(
        masker.mask("bad url https://api.test/?t=a%20b%26c and a b&c"),
        format!("bad url https://api.test/?t={MASK} and {MASK}")
    );
    let debug = format!("{masker:?}");
    assert!(
        !debug.contains("a b&c") && !debug.contains("a%20b"),
        "{debug}"
    );
}

#[test]
fn a_failed_send_error_never_echoes_a_query_secret() {
    let dir = common::temp_dir();
    let root = dir.path();
    common::write(
        root,
        "reqflux.toml",
        &format!(
            "id = \"{}\"\nname = \"W\"\nformat = 1\n",
            common::id_text(900)
        ),
    );
    common::write(
        root,
        "collections/api/collection.toml",
        &format!("id = \"{}\"\nname = \"Api\"\n", common::id_text(1)),
    );
    common::write(
        root,
        "collections/api/r.req.toml",
        &format!(
            "id = \"{}\"\nname = \"r\"\nmethod = \"GET\"\nurl = \"{{{{baseUrl}}}}/v1\"\n\n\
             [auth]\ntype = \"apikey\"\nkey = \"api_key\"\nvalue = \"k-SECRET-123\"\nlocation = \"query\"\n",
            common::id_text(2)
        ),
    );
    let engine = match common::open_engine(root) {
        Ok(engine) => engine,
        Err(error) => panic!("open: {error}"),
    };
    let execution = match engine.execute("api/r", &reqflux_engine::ExecuteOptions::default()) {
        Ok(execution) => execution,
        Err(error) => panic!("execute: {error}"),
    };
    let masker = execution.masker();

    let error = match engine.wait(execution) {
        Ok(executed) => panic!("expected a failure, got {}", executed.response.status),
        Err(error) => error.to_string(),
    };
    let masked = masker.mask(&error);

    assert!(
        error.contains("k-SECRET-123"),
        "the raw error echoes it: {error}"
    );
    assert!(masked.contains(MASK), "{masked}");
    assert!(!masked.contains("k-SECRET-123"), "{masked}");
}

#[test]
fn a_secret_in_a_header_name_is_masked() {
    let mut doc = post("https://api.test/");
    doc.headers = vec![KeyValue::new("X-{{token}}", "v")];
    let ws = workspace(
        vec![secret("token", "name-tok")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert!(
        sent.headers
            .iter()
            .all(|(key, value)| !key.contains("name-tok") && !value.contains("name-tok")),
        "{:?}",
        sent.headers
    );
    assert!(
        sent.headers.iter().any(|(key, _)| key == MASK),
        "{:?}",
        sent.headers
    );
}

#[test]
fn the_masker_masks_a_secret_in_the_url_crate_serialization() {
    let doc = post("https://api.test/v1?k={{token}}");
    let ws = workspace(
        vec![secret("token", "k SECRET/x")],
        vec![collection("c", vec![request("r", doc)])],
    );
    let masker = prepare(&ws).masker();
    let wire = match url::Url::parse("https://api.test/v1?k=k SECRET/x") {
        Ok(url) => url.to_string(),
        Err(error) => panic!("{error}"),
    };

    let masked = masker.mask(&wire);

    assert!(!masked.contains("SECRET"), "{masked}");
}

#[test]
fn a_secret_whose_url_form_is_empty_does_not_mask_everything() {
    // `#frag` serializes to an empty query and path: an empty pattern would match between
    // every character.
    let mut doc = post("https://api.test/v1");
    doc.headers = vec![KeyValue::new("X-Plain", "visible")];
    let ws = workspace(
        vec![secret("token", "#frag")],
        vec![collection("c", vec![request("r", doc)])],
    );

    let sent = prepare(&ws).sent();

    assert_eq!(sent.url, "https://api.test/v1");
    assert_eq!(header(&sent, "X-Plain"), "visible");
}

#[test]
fn the_masker_leaves_text_that_only_looks_like_user_info_alone() {
    let masker = reqflux_engine::Masker::default();
    for text in [
        "{\"url\":\"https://api.example.com\",\"user\":\"a@b.com\"}",
        "see https://example.com and mail user:me@example.org",
        "time https://h 10:30 by bob@x",
        "http://h/a:b@c",
        "http://h?q=a:b@c",
        "mailto:a:b@c.com",
        "git@github.com:org/repo.git",
        "http://h/x?next=https://app.test&at=10:30&by=bob@x.org",
        "[https://h](mailto:me@x.org)",
        "http://u:first@a/ then http://v:second@b/",
    ] {
        assert_eq!(masker.mask(text), text);
    }
}
