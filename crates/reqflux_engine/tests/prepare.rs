//! Request preparation without a network (S4-S10).

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{
    ROOT, collection, disabled, environment, folder, get, id, id_text, options, post, prepare,
    prepare_draft_err, prepare_draft_with, prepare_err, prepare_with, request, single, workspace,
};
use pretty_assertions::assert_eq;
use reqflux_core::model::{
    ApiKeyLocation, Auth, Body, FormPart, Item, KeyValue, RawLanguage, Settings, Variable,
};
use reqflux_engine::{DEFAULT_TIMEOUT_MS, ExecuteOptions};
use reqflux_http::{MultipartPart, MultipartValue, PreparedBody, RequestOptions};

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn secret(key: &str) -> Variable {
    Variable {
        value: None,
        secret: true,
        ..Variable::new(key, "")
    }
}

fn raw(language: RawLanguage, text: &str) -> Body {
    Body::Raw {
        language,
        text: text.to_owned(),
    }
}

fn bytes(content_type: Option<&str>, data: &str) -> PreparedBody {
    PreparedBody::Bytes {
        content_type: content_type.map(str::to_owned),
        data: data.as_bytes().to_vec(),
    }
}

// S4: scopes and precedence.

#[test]
fn s4_scopes_resolve_globals_collection_environment_local_in_precedence() {
    let mut ws = workspace(
        vec![
            Variable::new("g", "global"),
            Variable::new("gc", "global"),
            Variable::new("all", "global"),
        ],
        vec![collection(
            "c",
            vec![request(
                "r",
                get("https://h/{{g}}/{{gc}}/{{ce}}/{{el}}/{{all}}"),
            )],
        )],
    );
    if let Some(node) = ws.collections.first_mut() {
        node.collection.variables = vec![
            Variable::new("gc", "collection"),
            Variable::new("ce", "collection"),
            Variable::new("all", "collection"),
        ];
    }
    ws.environments.push(environment(
        1,
        "dev",
        vec![
            Variable::new("ce", "environment"),
            Variable::new("el", "environment"),
            Variable::new("all", "environment"),
        ],
    ));
    let prepared = prepare_with(
        &ws,
        "c/r",
        &options(Some("dev"), &[("el", "local"), ("all", "local")]),
    );
    assert_eq!(
        prepared.request.url,
        "https://h/global/collection/environment/local/local"
    );
    assert_eq!(prepared.unresolved, Vec::<String>::new());
}

#[test]
fn s4_later_var_with_the_same_key_wins() {
    let ws = single(get("https://h/{{a}}"));
    let prepared = prepare_with(&ws, "c/r", &options(None, &[("a", "1"), ("a", "2")]));
    assert_eq!(prepared.request.url, "https://h/2");
}

#[test]
fn s4_resolves_params_and_headers() {
    let mut doc = get("https://{{host}}/p");
    doc.params = vec![KeyValue::new("{{k}}", "{{v}}")];
    doc.headers = vec![KeyValue::new("X-{{k}}", "{{v}}")];
    let prepared = prepare_with(
        &single(doc),
        "c/r",
        &options(None, &[("host", "h"), ("k", "key"), ("v", "val")]),
    );
    assert_eq!(prepared.request.url, "https://h/p?key=val");
    assert_eq!(prepared.request.headers, headers(&[("X-key", "val")]));
}

#[test]
fn s4_resolves_raw_text() {
    let mut doc = post("https://h");
    doc.body = raw(RawLanguage::Text, "hello {{who}}");
    let prepared = prepare_with(&single(doc), "c/r", &options(None, &[("who", "you")]));
    assert_eq!(
        prepared.request.body,
        bytes(Some("text/plain"), "hello you")
    );
}

#[test]
fn s4_resolves_urlencoded_fields_and_skips_disabled_ones() {
    let mut doc = post("https://h");
    doc.body = Body::Urlencoded {
        fields: vec![
            KeyValue::new("{{k}}", "{{v}}"),
            disabled("off", "{{missing}}"),
        ],
    };
    let prepared = prepare_with(
        &single(doc),
        "c/r",
        &options(None, &[("k", "a"), ("v", "b")]),
    );
    assert_eq!(
        prepared.request.body,
        PreparedBody::Form(headers(&[("a", "b")]))
    );
    assert_eq!(prepared.unresolved, Vec::<String>::new());
}

#[test]
fn s4_resolves_formdata_keys_values_and_file_paths() {
    let mut doc = post("https://h");
    doc.body = Body::FormData {
        parts: vec![
            FormPart::Text {
                key: "{{k}}".to_owned(),
                value: "{{v}}".to_owned(),
                description: None,
                enabled: true,
            },
            FormPart::File {
                key: "file".to_owned(),
                path: "files/{{name}}".to_owned(),
                description: None,
                enabled: true,
            },
            FormPart::Text {
                key: "off".to_owned(),
                value: "{{missing}}".to_owned(),
                description: None,
                enabled: false,
            },
        ],
    };
    let prepared = prepare_with(
        &single(doc),
        "c/r",
        &options(None, &[("k", "a"), ("v", "b"), ("name", "x.txt")]),
    );
    assert_eq!(
        prepared.request.body,
        PreparedBody::Multipart(vec![
            MultipartPart {
                name: "a".to_owned(),
                value: MultipartValue::Text("b".to_owned()),
            },
            MultipartPart {
                name: "file".to_owned(),
                value: MultipartValue::File(PathBuf::from(ROOT).join("files/x.txt")),
            },
        ])
    );
    assert_eq!(prepared.unresolved, Vec::<String>::new());
}

#[test]
fn s4_resolves_graphql_query_and_variables() {
    let mut doc = post("https://h");
    doc.body = Body::Graphql {
        query: "query { {{field}} }".to_owned(),
        variables: r#"{"id": "{{id}}"}"#.to_owned(),
    };
    let prepared = prepare_with(
        &single(doc),
        "c/r",
        &options(None, &[("field", "user"), ("id", "7")]),
    );
    assert_eq!(
        prepared.request.body,
        bytes(
            Some("application/json"),
            r#"{"query":"query { user }","variables":{"id":"7"}}"#
        )
    );
}

#[test]
fn s4_resolves_auth_fields() {
    let mut doc = get("https://h");
    doc.auth = Auth::Bearer {
        token: "{{token}}".to_owned(),
    };
    let prepared = prepare_with(&single(doc), "c/r", &options(None, &[("token", "t1")]));
    assert_eq!(
        prepared.request.headers,
        headers(&[("Authorization", "Bearer t1")])
    );
}

#[test]
fn s4_disabled_rows_are_skipped_entirely() {
    let mut doc = get("https://h/p");
    doc.params = vec![disabled("q", "{{a}}")];
    doc.headers = vec![disabled("X-A", "{{b}}")];
    let prepared = prepare(&single(doc));
    assert_eq!(prepared.request.url, "https://h/p");
    assert_eq!(prepared.request.headers, headers(&[]));
    assert_eq!(prepared.unresolved, Vec::<String>::new());
}

#[test]
fn blank_param_and_header_rows_are_not_sent() {
    let mut doc = get("dsadsadsad");
    doc.params = vec![KeyValue::new("", "")];
    doc.headers = vec![KeyValue::new("", "")];
    let ws = single(doc.clone());
    let prepared = prepare(&ws);
    assert_eq!(prepared.request.url, "dsadsadsad");
    assert_eq!(prepared.request.headers, headers(&[]));
    let Some(Item::Request(node)) = ws.collections.first().and_then(|node| node.items.first())
    else {
        panic!("the request is in the workspace");
    };
    assert_eq!(node.request, doc);
}

#[test]
fn rows_with_only_a_key_or_a_value_are_still_sent() {
    let mut doc = get("https://h/p");
    doc.params = vec![KeyValue::new("flag", ""), KeyValue::new("", "v")];
    let prepared = prepare(&single(doc));
    assert_eq!(prepared.request.url, "https://h/p?flag=&=v");
}

#[test]
fn s4_secret_without_value_does_not_resolve() {
    let ws = workspace(
        vec![secret("token")],
        vec![collection(
            "c",
            vec![request("r", get("https://h/{{token}}"))],
        )],
    );
    let prepared = prepare(&ws);
    assert_eq!(prepared.request.url, "https://h/{{token}}");
    assert_eq!(prepared.unresolved, vec!["token".to_owned()]);
}

#[test]
fn s4_secret_resolves_from_a_local_var() {
    let ws = workspace(
        vec![secret("token")],
        vec![collection(
            "c",
            vec![request("r", get("https://h/{{token}}"))],
        )],
    );
    let prepared = prepare_with(&ws, "c/r", &options(None, &[("token", "s")]));
    assert_eq!(prepared.request.url, "https://h/s");
}

#[test]
fn s4_unresolved_names_are_reported_once_in_first_occurrence_order() {
    let mut doc = post("https://h/{{b}}");
    doc.params = vec![KeyValue::new("q", "{{a}}")];
    doc.headers = vec![KeyValue::new("X-B", "{{b}}"), KeyValue::new("X-C", "{{c}}")];
    doc.body = raw(RawLanguage::Text, "{{a}}{{d}}");
    let prepared = prepare(&single(doc));
    assert_eq!(prepared.unresolved, vec!["b", "a", "c", "d"]);
    assert_eq!(prepared.request.url, "https://h/{{b}}?q=%7B%7Ba%7D%7D");
    assert_eq!(
        prepared.request.headers,
        headers(&[("X-B", "{{b}}"), ("X-C", "{{c}}")])
    );
}

#[test]
fn s4_dynamic_variables_resolve() {
    let prepared = prepare(&single(get("https://h/{{$guid}}")));
    assert_eq!(
        prepared.request.url,
        "https://h/00000000-0000-4000-8000-000000000001"
    );
}

// S5: environments.

#[test]
fn s5_unknown_environment_fails() {
    let ws = single(get("https://h"));
    assert_eq!(
        prepare_err(&ws, "c/r", &options(Some("prod"), &[])),
        "unknown environment prod"
    );
}

#[test]
fn s5_environment_outside_the_allowlist_fails() {
    let mut ws = single(get("https://h"));
    ws.environments.push(environment(1, "dev", Vec::new()));
    ws.environments.push(environment(2, "prod", Vec::new()));
    if let Some(node) = ws.collections.first_mut() {
        node.collection.environments = vec![id(1)];
    }
    assert_eq!(
        prepare_err(&ws, "c/r", &options(Some("prod"), &[])),
        "environment prod is not allowed for collection c name"
    );
    let allowed = prepare_with(&ws, "c/r", &options(Some("dev"), &[]));
    assert_eq!(allowed.request.url, "https://h");
}

#[test]
fn s5_no_environment_means_no_environment_scope() {
    let mut ws = single(get("https://h/{{e}}"));
    ws.environments
        .push(environment(1, "dev", vec![Variable::new("e", "x")]));
    let prepared = prepare(&ws);
    assert_eq!(prepared.request.url, "https://h/{{e}}");
    assert_eq!(prepared.unresolved, vec!["e".to_owned()]);
}

// S6: query string.

#[test]
fn s6_params_are_percent_encoded_in_order() {
    let mut doc = get("https://h/p");
    doc.params = vec![
        KeyValue::new("q", "a b&c=d/é"),
        KeyValue::new("empty", ""),
        KeyValue::new("x y", "1"),
    ];
    assert_eq!(
        prepare(&single(doc)).request.url,
        "https://h/p?q=a%20b%26c%3Dd%2F%C3%A9&empty=&x%20y=1"
    );
}

#[test]
fn s6_params_append_with_ampersand_when_the_url_has_a_query() {
    let mut doc = get("https://h/p?fixed=1");
    doc.params = vec![KeyValue::new("q", "2")];
    assert_eq!(prepare(&single(doc)).request.url, "https://h/p?fixed=1&q=2");
}

#[test]
fn s6_no_params_leave_the_url_unchanged() {
    assert_eq!(
        prepare(&single(get("https://h/p"))).request.url,
        "https://h/p"
    );
}

// S7: auth.

fn bearer(token: &str) -> Auth {
    Auth::Bearer {
        token: token.to_owned(),
    }
}

#[test]
fn s7_inherit_walks_folders_up_to_the_collection() {
    let mut node = collection(
        "c",
        vec![folder(
            "outer",
            bearer("outer"),
            vec![folder(
                "inner",
                Auth::Inherit,
                vec![request("r", get("https://h"))],
            )],
        )],
    );
    node.collection.auth = bearer("collection");
    let ws = workspace(Vec::new(), vec![node]);
    let prepared = prepare_with(&ws, "c/outer/inner/r", &ExecuteOptions::default());
    assert_eq!(
        prepared.request.headers,
        headers(&[("Authorization", "Bearer outer")])
    );
}

#[test]
fn s7_inherit_reaches_the_collection_auth() {
    let mut node = collection(
        "c",
        vec![folder(
            "f",
            Auth::Inherit,
            vec![request("r", get("https://h"))],
        )],
    );
    node.collection.auth = bearer("collection");
    let ws = workspace(Vec::new(), vec![node]);
    let prepared = prepare_with(&ws, "c/f/r", &ExecuteOptions::default());
    assert_eq!(
        prepared.request.headers,
        headers(&[("Authorization", "Bearer collection")])
    );
}

#[test]
fn s7_a_collection_that_inherits_sends_no_auth() {
    let ws = workspace(
        Vec::new(),
        vec![collection(
            "c",
            vec![folder(
                "f",
                Auth::Inherit,
                vec![request("r", get("https://h"))],
            )],
        )],
    );
    let prepared = prepare_with(&ws, "c/f/r", &ExecuteOptions::default());
    assert_eq!(prepared.request.headers, headers(&[]));
}

#[test]
fn s7_none_stops_the_walk() {
    let mut node = collection(
        "c",
        vec![folder(
            "f",
            Auth::None,
            vec![request("r", get("https://h"))],
        )],
    );
    node.collection.auth = bearer("collection");
    let ws = workspace(Vec::new(), vec![node]);
    let prepared = prepare_with(&ws, "c/f/r", &ExecuteOptions::default());
    assert_eq!(prepared.request.headers, headers(&[]));
}

#[test]
fn s7_basic_sends_base64_credentials_after_request_headers() {
    let mut doc = get("https://h");
    doc.headers = vec![KeyValue::new("Accept", "*/*")];
    doc.auth = Auth::Basic {
        username: "{{user}}".to_owned(),
        password: "p:ss".to_owned(),
    };
    let prepared = prepare_with(&single(doc), "c/r", &options(None, &[("user", "ana")]));
    assert_eq!(
        prepared.request.headers,
        headers(&[("Accept", "*/*"), ("Authorization", "Basic YW5hOnA6c3M=")])
    );
}

#[test]
fn s7_apikey_in_a_header() {
    let mut doc = get("https://h");
    doc.auth = Auth::ApiKey {
        key: "X-Api-Key".to_owned(),
        value: "{{key}}".to_owned(),
        location: ApiKeyLocation::Header,
    };
    let prepared = prepare_with(&single(doc), "c/r", &options(None, &[("key", "k1")]));
    assert_eq!(prepared.request.headers, headers(&[("X-Api-Key", "k1")]));
    assert_eq!(prepared.request.url, "https://h");
}

#[test]
fn s7_apikey_in_the_query_after_params() {
    let mut doc = get("https://h/p");
    doc.params = vec![KeyValue::new("a", "1")];
    doc.auth = Auth::ApiKey {
        key: "api key".to_owned(),
        value: "v&1".to_owned(),
        location: ApiKeyLocation::Query,
    };
    let prepared = prepare(&single(doc));
    assert_eq!(prepared.request.url, "https://h/p?a=1&api%20key=v%261");
    assert_eq!(prepared.request.headers, headers(&[]));
}

#[test]
fn s7_an_enabled_request_header_wins_case_insensitively() {
    let mut doc = get("https://h");
    doc.headers = vec![KeyValue::new("authorization", "Custom x")];
    doc.auth = Auth::Basic {
        username: "{{unused}}".to_owned(),
        password: String::new(),
    };
    let prepared = prepare(&single(doc));
    assert_eq!(
        prepared.request.headers,
        headers(&[("authorization", "Custom x")])
    );
    assert_eq!(prepared.unresolved, Vec::<String>::new());
}

#[test]
fn s7_a_disabled_request_header_does_not_win() {
    let mut doc = get("https://h");
    doc.headers = vec![disabled("Authorization", "Custom x")];
    doc.auth = bearer("t");
    let prepared = prepare(&single(doc));
    assert_eq!(
        prepared.request.headers,
        headers(&[("Authorization", "Bearer t")])
    );
}

#[test]
fn s7_other_auth_types_are_not_supported() {
    let mut node = collection("c", vec![request("r", get("https://h"))]);
    node.collection.auth = Auth::Other {
        kind: "digest".to_owned(),
        fields: toml::Table::new(),
    };
    let ws = workspace(Vec::new(), vec![node]);
    assert_eq!(
        prepare_err(&ws, "c/r", &ExecuteOptions::default()),
        "auth type digest is not supported yet"
    );
}

// S8: bodies.

#[test]
fn s8_raw_content_type_follows_the_language() {
    for (language, content_type) in [
        (RawLanguage::Json, "application/json"),
        (RawLanguage::Xml, "application/xml"),
        (RawLanguage::Text, "text/plain"),
        (RawLanguage::Html, "text/html"),
        (RawLanguage::Javascript, "application/javascript"),
    ] {
        let mut doc = post("https://h");
        doc.body = raw(language, "x");
        assert_eq!(
            prepare(&single(doc)).request.body,
            bytes(Some(content_type), "x")
        );
    }
}

#[test]
fn s8_an_enabled_content_type_header_replaces_the_language_default() {
    let mut doc = post("https://h");
    doc.headers = vec![KeyValue::new("content-type", "application/vnd.x+json")];
    doc.body = raw(RawLanguage::Json, "{}");
    let prepared = prepare(&single(doc));
    assert_eq!(prepared.request.body, bytes(None, "{}"));
    assert_eq!(
        prepared.request.headers,
        headers(&[("content-type", "application/vnd.x+json")])
    );
}

#[test]
fn s8_a_disabled_content_type_header_keeps_the_language_default() {
    let mut doc = post("https://h");
    doc.headers = vec![disabled("Content-Type", "text/csv")];
    doc.body = raw(RawLanguage::Xml, "<a/>");
    assert_eq!(
        prepare(&single(doc)).request.body,
        bytes(Some("application/xml"), "<a/>")
    );
}

#[test]
fn s8_graphql_with_empty_variables_sends_null() {
    let mut doc = post("https://h");
    doc.body = Body::Graphql {
        query: "{ me }".to_owned(),
        variables: "  ".to_owned(),
    };
    assert_eq!(
        prepare(&single(doc)).request.body,
        bytes(
            Some("application/json"),
            r#"{"query":"{ me }","variables":null}"#
        )
    );
}

#[test]
fn s8_graphql_with_invalid_variables_fails() {
    let mut doc = post("https://h");
    doc.body = Body::Graphql {
        query: "{ me }".to_owned(),
        variables: "{oops".to_owned(),
    };
    assert_eq!(
        prepare_err(&single(doc), "c/r", &ExecuteOptions::default()),
        "graphql variables are not valid JSON: key must be a string at line 1 column 2"
    );
}

#[test]
fn s8_binary_streams_a_file_relative_to_the_root() {
    let mut doc = post("https://h");
    doc.body = Body::Binary {
        path: "data/{{file}}".to_owned(),
    };
    let prepared = prepare_with(&single(doc), "c/r", &options(None, &[("file", "a.bin")]));
    assert_eq!(
        prepared.request.body,
        PreparedBody::File(PathBuf::from(ROOT).join("data/a.bin"))
    );
}

#[test]
fn s8_absolute_file_paths_are_kept() {
    let mut doc = post("https://h");
    doc.body = Body::Binary {
        path: "/tmp/a.bin".to_owned(),
    };
    assert_eq!(
        prepare(&single(doc)).request.body,
        PreparedBody::File(PathBuf::from("/tmp/a.bin"))
    );
}

#[test]
fn s8_no_body_sends_nothing() {
    assert_eq!(
        prepare(&single(get("https://h"))).request.body,
        PreparedBody::None
    );
}

// S9: settings.

#[test]
fn s9_unset_settings_use_the_defaults() {
    let prepared = prepare(&single(get("https://h")));
    assert_eq!(DEFAULT_TIMEOUT_MS, 30_000);
    assert_eq!(
        prepared.request.options,
        RequestOptions {
            timeout: Duration::from_millis(30_000),
            follow_redirects: true,
            verify_tls: true,
        }
    );
}

#[test]
fn s9_set_settings_are_used() {
    let mut doc = get("https://h");
    doc.settings = Settings {
        timeout_ms: Some(1500),
        follow_redirects: Some(false),
        verify_tls: Some(false),
    };
    assert_eq!(
        prepare(&single(doc)).request.options,
        RequestOptions {
            timeout: Duration::from_millis(1500),
            follow_redirects: false,
            verify_tls: false,
        }
    );
}

#[test]
fn s9_method_is_the_request_verb() {
    let mut doc = get("https://h");
    doc.method = reqflux_core::model::Method::new("purge").unwrap_or_default();
    assert_eq!(prepare(&single(doc)).request.method, "PURGE");
}

// S10: selection.

#[test]
fn s10_selects_by_id() {
    let mut doc = get("https://h/by-id");
    doc.id = id(5);
    let ws = workspace(
        Vec::new(),
        vec![collection(
            "c",
            vec![folder("f", Auth::Inherit, vec![request("r", doc)])],
        )],
    );
    let prepared = prepare_with(&ws, &id_text(5), &ExecuteOptions::default());
    assert_eq!(prepared.request.url, "https://h/by-id");
}

#[test]
fn s10_selects_by_slug_path() {
    let ws = workspace(
        Vec::new(),
        vec![
            collection("a", vec![request("r", get("https://h/a"))]),
            collection(
                "c",
                vec![
                    request("r", get("https://h/top")),
                    folder(
                        "f",
                        Auth::Inherit,
                        vec![request("r", get("https://h/nested"))],
                    ),
                ],
            ),
        ],
    );
    let opts = ExecuteOptions::default();
    assert_eq!(
        prepare_with(&ws, "c/f/r", &opts).request.url,
        "https://h/nested"
    );
    assert_eq!(prepare_with(&ws, "c/r", &opts).request.url, "https://h/top");
}

#[test]
fn s10_no_match_fails() {
    let ws = workspace(
        Vec::new(),
        vec![collection(
            "c",
            vec![folder(
                "f",
                Auth::Inherit,
                vec![request("r", get("https://h"))],
            )],
        )],
    );
    let opts = ExecuteOptions::default();
    for selector in ["c/x", "c/f", "c", "c/r", "x/f/r", ""] {
        assert_eq!(
            prepare_err(&ws, selector, &opts),
            format!("no request matches {selector}")
        );
    }
}

// S5 (request-editing): preparing a draft.

#[test]
fn s5_draft_url_params_headers_auth_and_body_are_the_ones_prepared() {
    let ws = single(get("https://stored/{{a}}"));
    let mut draft = post("https://draft/{{b}}");
    draft.params = vec![KeyValue::new("q", "{{c}}")];
    draft.headers = vec![KeyValue::new("X-Draft", "{{d}}")];
    draft.auth = Auth::Bearer {
        token: "{{t}}".to_owned(),
    };
    draft.body = raw(RawLanguage::Text, "hello {{e}}");
    let prepared = prepare_draft_with(
        &ws,
        "c/r",
        &draft,
        &options(
            None,
            &[("b", "B"), ("c", "C"), ("d", "D"), ("e", "E"), ("t", "T")],
        ),
    );
    assert_eq!(prepared.request.method, "POST");
    assert_eq!(prepared.request.url, "https://draft/B?q=C");
    assert_eq!(
        prepared.request.headers,
        headers(&[("X-Draft", "D"), ("Authorization", "Bearer T")])
    );
    assert_eq!(prepared.request.body, bytes(Some("text/plain"), "hello E"));
    let stored = ws
        .collections
        .iter()
        .flat_map(|node| &node.items)
        .find_map(|item| match item {
            Item::Request(node) => Some(&node.request),
            Item::Folder(_) => None,
        });
    let Some(stored) = stored else {
        panic!("the stored request disappeared");
    };
    assert_eq!(stored.method.as_str(), "GET");
    assert_eq!(stored.url, "https://stored/{{a}}");
    assert_eq!(stored.params, Vec::<KeyValue>::new());
    assert_eq!(stored.headers, Vec::<KeyValue>::new());
    assert_eq!(stored.auth, Auth::Inherit);
    assert_eq!(stored.body, Body::None);
}

#[test]
fn s5_draft_inherits_variables_auth_and_the_environment_allowlist() {
    let mut node = collection(
        "c",
        vec![folder(
            "f",
            bearer("folder"),
            vec![request("r", get("https://h"))],
        )],
    );
    node.collection.variables = vec![Variable::new("cv", "from-collection")];
    node.collection.auth = bearer("collection");
    node.collection.environments = vec![id(1)];
    let mut ws = workspace(Vec::new(), vec![node]);
    ws.environments
        .push(environment(1, "dev", vec![Variable::new("ev", "env")]));
    ws.environments
        .push(environment(2, "prod", vec![Variable::new("ev", "prod")]));
    let draft = get("https://{{cv}}/{{ev}}");
    let prepared = prepare_draft_with(&ws, "c/f/r", &draft, &options(Some("dev"), &[]));
    assert_eq!(prepared.request.url, "https://from-collection/env");
    assert_eq!(
        prepared.request.headers,
        headers(&[("Authorization", "Bearer folder")])
    );
    assert_eq!(
        prepare_draft_err(&ws, "c/f/r", &draft, &options(Some("prod"), &[])),
        "environment prod is not allowed for collection c name"
    );
}

#[test]
fn s5_draft_with_an_unknown_selector_still_fails() {
    let ws = single(get("https://h"));
    assert_eq!(
        prepare_draft_err(&ws, "c/x", &get("https://h"), &ExecuteOptions::default()),
        "no request matches c/x"
    );
}

#[test]
fn s5_draft_with_invalid_graphql_variables_reports_the_stored_error() {
    let mut draft = post("https://h");
    draft.body = Body::Graphql {
        query: "{ me }".to_owned(),
        variables: "{oops".to_owned(),
    };
    assert_eq!(
        prepare_draft_err(
            &single(get("https://h")),
            "c/r",
            &draft,
            &ExecuteOptions::default()
        ),
        "graphql variables are not valid JSON: key must be a string at line 1 column 2"
    );
}
