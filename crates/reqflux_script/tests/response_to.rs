//! The chai-style `pm.response.to` chain, run as real scripts (T19, S23).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::collections::BTreeMap;

use reqflux_script::{RequestView, ResponseView, ScriptContext, ScriptStage, TestResult, run};

fn context(code: u16, status: &str, body: &str) -> ScriptContext {
    ScriptContext {
        stage: ScriptStage::Test,
        request: RequestView {
            method: "GET".into(),
            url: "https://api.example.com/".into(),
            headers: BTreeMap::new(),
            body: None,
        },
        variables: BTreeMap::new(),
        response: Some(ResponseView {
            code,
            status: status.into(),
            response_time_ms: 5,
            response_size: body.len() as u64,
            headers: BTreeMap::from([
                ("Content-Type".into(), "application/json".into()),
                ("X-Trace".into(), "abc".into()),
            ]),
            body_text: body.into(),
            parsed_json: serde_json::from_str(body).ok(),
        }),
        send_request: None,
    }
}

/// Runs one assertion body inside a `pm.test` and returns its result.
fn check(context: &ScriptContext, body: &str) -> TestResult {
    let script = format!("pm.test(\"t\", () => {{ {body} }});");
    let outcome = run(&script, context).unwrap_or_else(|error| panic!("script failed: {error}"));
    outcome.tests.into_iter().next().unwrap()
}

fn passes(context: &ScriptContext, body: &str) {
    let result = check(context, body);
    assert!(result.passed, "{body}: {:?}", result.message);
}

fn fails_with(context: &ScriptContext, body: &str, message: &str) {
    let result = check(context, body);
    assert!(!result.passed, "{body} should fail");
    assert_eq!(result.message.as_deref(), Some(message), "{body}");
}

#[test]
fn status_by_code_and_by_reason() {
    let ok = context(200, "OK", "{}");
    passes(&ok, "pm.response.to.have.status(200);");
    passes(&ok, "pm.response.to.have.status('OK');");
    fails_with(
        &ok,
        "pm.response.to.have.status(404);",
        "expected response to have status code 404 but got 200",
    );
    fails_with(
        &ok,
        "pm.response.to.have.status('Not Found');",
        "expected response to have status reason 'Not Found' but got 'OK'",
    );
}

#[test]
fn header_by_name_and_value_ignores_name_case() {
    let ok = context(200, "OK", "{}");
    passes(&ok, "pm.response.to.have.header('content-type');");
    passes(&ok, "pm.response.to.have.header('X-Trace', 'abc');");
    fails_with(
        &ok,
        "pm.response.to.have.header('X-Missing');",
        "expected response to have header with key 'X-Missing'",
    );
    fails_with(
        &ok,
        "pm.response.to.have.header('X-Trace', 'zzz');",
        "expected response to have header with key 'X-Trace' and value 'zzz', got 'abc'",
    );
}

#[test]
fn body_text_and_json_body() {
    let ok = context(200, "OK", r#"{"id":7,"tags":["a"]}"#);
    passes(&ok, r#"pm.response.to.have.body('{"id":7,"tags":["a"]}');"#);
    passes(&ok, "pm.response.to.have.body(/\"id\"/);");
    fails_with(
        &ok,
        "pm.response.to.have.body('nope');",
        r#"expected response body to equal 'nope' but got '{"id":7,"tags":["a"]}'"#,
    );
    passes(&ok, "pm.response.to.have.jsonBody();");
    passes(&ok, "pm.response.to.have.jsonBody({ id: 7, tags: ['a'] });");
    passes(&ok, "pm.response.to.have.jsonBody('id', 7);");
    passes(&ok, "pm.response.to.have.jsonBody('tags.0', 'a');");
    fails_with(
        &ok,
        "pm.response.to.have.jsonBody('id', 8);",
        "expected response json body at 'id' to equal 8 but got 7",
    );
    let text = context(200, "OK", "plain");
    fails_with(
        &text,
        "pm.response.to.have.jsonBody();",
        "expected response body to be valid JSON",
    );
}

#[test]
fn status_family_flags() {
    let ok = context(200, "OK", "{}");
    passes(&ok, "pm.response.to.be.ok;");
    passes(&ok, "pm.response.to.be.success;");
    passes(&ok, "pm.response.to.be.json;");
    fails_with(
        &ok,
        "pm.response.to.be.error;",
        "expected response to be an error (status 4xx or 5xx) but got 200",
    );
    let created = context(201, "Created", "{}");
    passes(&created, "pm.response.to.be.success;");
    fails_with(
        &created,
        "pm.response.to.be.ok;",
        "expected response to be ok (status 200) but got 201",
    );
    let missing = context(404, "Not Found", "{}");
    passes(&missing, "pm.response.to.be.error;");
    fails_with(
        &missing,
        "pm.response.to.be.success;",
        "expected response to be successful (status 2xx) but got 404",
    );
    let text = context(200, "OK", "plain");
    fails_with(
        &text,
        "pm.response.to.be.json;",
        "expected response body to be valid JSON",
    );
}

#[test]
fn not_negates_every_assertion() {
    let ok = context(200, "OK", "{}");
    passes(&ok, "pm.response.to.not.have.status(404);");
    passes(&ok, "pm.response.to.not.have.header('X-Missing');");
    passes(&ok, "pm.response.to.not.be.error;");
    passes(&ok, "pm.response.to.have.not.body('x');");
    fails_with(
        &ok,
        "pm.response.to.not.have.status(200);",
        "expected response to not have status code 200",
    );
    fails_with(
        &ok,
        "pm.response.to.not.be.ok;",
        "expected response to not be ok (status 200)",
    );
    fails_with(
        &ok,
        "pm.response.to.not.have.header('X-Trace');",
        "expected response to not have header with key 'X-Trace'",
    );
}

#[test]
fn an_unknown_link_throws_instead_of_passing() {
    let ok = context(200, "OK", "{}");
    let result = check(&ok, "pm.response.to.have.nothing(1);");
    assert!(!result.passed);
}
