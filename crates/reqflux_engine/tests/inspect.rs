//! What a front end shows about a request before sending it, with or without an environment.

#![allow(clippy::panic, reason = "test helpers fail the calling test")]

mod common;

use std::path::Path;

use common::{ROOT, collection, disabled, environment, get, id, post, request, single, workspace};
use pretty_assertions::assert_eq;
use reqflux_core::model::{Auth, KeyValue, Variable, Workspace};
use reqflux_engine::{Inspection, Span};

fn inspect(workspace: &Workspace) -> Inspection {
    inspect_in(workspace, None)
}

fn inspect_in(workspace: &Workspace, environment: Option<&str>) -> Inspection {
    match reqflux_engine::inspect(workspace, Path::new(ROOT), "c/r", environment) {
        Ok(inspection) => inspection,
        Err(error) => panic!("inspect failed: {error}"),
    }
}

fn secret(key: &str, value: &str) -> Variable {
    Variable {
        secret: true,
        ..Variable::new(key, value)
    }
}

fn literal(text: &str) -> Span {
    Span::Literal(text.to_owned())
}

fn reference(name: &str, resolves: bool, secret: bool) -> Span {
    Span::Reference {
        raw: format!("{{{{{name}}}}}"),
        name: name.to_owned(),
        resolves,
        secret,
    }
}

/// A workspace whose collection `c` declares `variables` and holds `c/r` with `url`.
fn with_collection_variables(url: &str, variables: Vec<Variable>) -> Workspace {
    let mut ws = single(get(url));
    if let Some(node) = ws.collections.first_mut() {
        node.collection.variables = variables;
    }
    ws
}

#[test]
fn url_appends_enabled_params_unencoded_after_a_question_mark() {
    let mut request_doc = get("{{base}}/v1/invoices");
    request_doc.params = vec![
        KeyValue::new("expand", "lines"),
        disabled("skip", "me"),
        KeyValue::new("q", "a b"),
    ];
    let inspection = inspect(&single(request_doc));
    assert_eq!(inspection.url, "{{base}}/v1/invoices?expand=lines&q=a b");
}

#[test]
fn url_skips_a_param_row_with_an_empty_key_and_value() {
    let mut request_doc = get("dsadsadsad");
    request_doc.params = vec![KeyValue::new("", "")];
    let inspection = inspect(&single(request_doc));
    assert_eq!(inspection.url, "dsadsadsad");
}

#[test]
fn url_appends_params_after_an_ampersand_when_it_has_a_query() {
    let mut request_doc = get("https://h/p?a=1");
    request_doc.params = vec![KeyValue::new("b", "{{two}}")];
    let inspection = inspect(&single(request_doc));
    assert_eq!(inspection.url, "https://h/p?a=1&b={{two}}");
}

#[test]
fn url_without_enabled_params_is_the_request_url() {
    let mut request_doc = get("https://h/p");
    request_doc.params = vec![disabled("a", "1")];
    assert_eq!(inspect(&single(request_doc)).url, "https://h/p");
}

#[test]
fn unresolved_lists_names_no_scope_defines_without_an_environment() {
    let mut ws = with_collection_variables(
        "{{known}}/{{env_only}}/{{missing}}/{{$guid}}",
        vec![Variable::new("known", "k")],
    );
    ws.environments
        .push(environment(1, "dev", vec![Variable::new("env_only", "e")]));
    let inspection = inspect(&ws);
    assert_eq!(inspection.unresolved, vec!["env_only", "missing"]);
}

#[test]
fn unresolved_is_empty_when_the_request_cannot_be_prepared() {
    let mut request_doc = get("{{missing}}");
    request_doc.auth = Auth::Other {
        kind: "oauth2".to_owned(),
        fields: toml::Table::new(),
    };
    let inspection = inspect(&single(request_doc));
    assert_eq!(inspection.unresolved, Vec::<String>::new());
    assert_eq!(inspection.url, "{{missing}}");
    assert_eq!(
        inspection.problem.as_deref(),
        Some("auth type oauth2 is not supported yet")
    );
}

#[test]
fn a_preparable_request_has_no_problem() {
    assert_eq!(inspect(&single(get("u"))).problem, None);
}

/// `c/r` at `{{base}}/{{env_only}}/{{missing}}` with `base` in the collection and
/// `env_only` in the environment `dev`.
fn with_dev_environment() -> Workspace {
    let mut ws = with_collection_variables(
        "{{base}}/{{env_only}}/{{missing}}",
        vec![Variable::new("base", "b")],
    );
    ws.environments
        .push(environment(1, "dev", vec![Variable::new("env_only", "e")]));
    ws
}

#[test]
fn an_environment_resolves_its_variables_in_spans_and_unresolved() {
    let inspection = inspect_in(&with_dev_environment(), Some("dev"));
    assert_eq!(inspection.unresolved, vec!["missing"]);
    assert_eq!(inspection.problem, None);
    assert_eq!(
        inspection.spans("{{env_only}}{{missing}}"),
        vec![
            reference("env_only", true, false),
            reference("missing", false, false),
        ]
    );
}

#[test]
fn an_environment_the_collection_does_not_allow_is_a_problem() {
    let mut ws = with_dev_environment();
    if let Some(node) = ws.collections.first_mut() {
        node.collection.environments = vec![id(2)];
    }
    let inspection = inspect_in(&ws, Some("dev"));
    assert_eq!(
        inspection.problem.as_deref(),
        Some("environment dev is not allowed for collection c name")
    );
    assert_eq!(inspection.unresolved, Vec::<String>::new());
    assert_eq!(
        inspection.spans("{{base}}{{env_only}}"),
        vec![
            reference("base", true, false),
            reference("env_only", false, false)
        ]
    );
}

#[test]
fn an_allowed_environment_still_resolves_when_the_auth_is_the_problem() {
    let mut ws = with_dev_environment();
    if let Some(request_doc) = ws
        .collections
        .first_mut()
        .and_then(|node| node.items.first_mut())
        .and_then(|item| match item {
            reqflux_core::model::Item::Request(node) => Some(&mut node.request),
            reqflux_core::model::Item::Folder(_) => None,
        })
    {
        request_doc.auth = Auth::Other {
            kind: "oauth2".to_owned(),
            fields: toml::Table::new(),
        };
    }
    let inspection = inspect_in(&ws, Some("dev"));
    assert_eq!(
        inspection.problem.as_deref(),
        Some("auth type oauth2 is not supported yet")
    );
    assert_eq!(
        inspection.spans("{{env_only}}"),
        vec![reference("env_only", true, false)]
    );
}

#[test]
fn an_unknown_environment_is_a_problem() {
    let inspection = inspect_in(&with_dev_environment(), Some("nope"));
    assert_eq!(
        inspection.problem.as_deref(),
        Some("unknown environment nope")
    );
    assert_eq!(inspection.unresolved, Vec::<String>::new());
    assert_eq!(
        inspection.spans("{{env_only}}"),
        vec![reference("env_only", false, false)]
    );
}

#[test]
fn spans_split_text_and_mark_references_that_resolve() {
    let mut ws = workspace(
        vec![Variable::new("global", "g")],
        vec![collection("c", vec![request("r", get("u"))])],
    );
    if let Some(node) = ws.collections.first_mut() {
        node.collection.variables = vec![Variable::new("col", "c")];
    }
    let inspection = inspect(&ws);
    assert_eq!(
        inspection.spans("a{{global}}b{{ col }}{{$guid}}{{nope}}{{"),
        vec![
            literal("a"),
            reference("global", true, false),
            literal("b"),
            Span::Reference {
                raw: "{{ col }}".to_owned(),
                name: "col".to_owned(),
                resolves: true,
                secret: false,
            },
            reference("$guid", true, false),
            reference("nope", false, false),
            literal("{{"),
        ]
    );
}

#[test]
fn spans_mark_secrets_of_globals_the_collection_and_allowed_environments() {
    let mut ws = with_collection_variables("u", vec![secret("col_secret", "c")]);
    ws.manifest.globals = vec![secret("global_secret", "g")];
    ws.environments
        .push(environment(1, "dev", vec![secret("env_secret", "")]));
    ws.environments
        .push(environment(2, "other", vec![secret("other_secret", "o")]));
    if let Some(node) = ws.collections.first_mut() {
        node.collection.environments = vec![id(1)];
    }
    let inspection = inspect(&ws);
    assert_eq!(
        inspection.spans("{{global_secret}}{{col_secret}}{{env_secret}}{{other_secret}}"),
        vec![
            reference("global_secret", true, true),
            reference("col_secret", true, true),
            reference("env_secret", false, true),
            reference("other_secret", false, false),
        ]
    );
}

#[test]
fn references_secret_tells_whether_any_reference_is_secret() {
    let inspection = inspect(&with_collection_variables(
        "u",
        vec![secret("token", "t"), Variable::new("plain", "p")],
    ));
    assert!(inspection.references_secret("Bearer {{token}}"));
    assert!(!inspection.references_secret("{{plain}} token"));
}

#[test]
fn an_unknown_selector_is_an_error() {
    match reqflux_engine::inspect(&single(get("u")), Path::new(ROOT), "c/none", None) {
        Ok(inspection) => panic!("inspect unexpectedly succeeded: {inspection:?}"),
        Err(error) => assert_eq!(error.to_string(), "no request matches c/none"),
    }
}

#[test]
fn the_inspected_request_is_the_selected_one() {
    let ws = workspace(
        Vec::new(),
        vec![collection(
            "c",
            vec![
                request("q", post("https://q")),
                request("r", get("https://r")),
            ],
        )],
    );
    let inspection = inspect(&ws);
    assert_eq!(inspection.request.slug, "r");
    assert_eq!(inspection.request.request.url, "https://r");
}
