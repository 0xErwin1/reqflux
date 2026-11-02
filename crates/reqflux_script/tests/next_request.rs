//! `pm.setNextRequest` and `postman.setNextRequest` for the collection runner (V1 S1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

use std::collections::BTreeMap;

use reqflux_script::{
    NextRequest, RequestView, ResponseView, ScriptContext, ScriptErrorKind, ScriptOutcome,
    ScriptStage, run,
};

fn context(stage: ScriptStage) -> ScriptContext {
    let response = (stage == ScriptStage::Test).then(|| ResponseView {
        code: 200,
        status: "OK".into(),
        response_time_ms: 1,
        response_size: 2,
        headers: BTreeMap::new(),
        body_text: "{}".into(),
        parsed_json: None,
    });
    ScriptContext {
        stage,
        request: RequestView {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: BTreeMap::new(),
            body: None,
        },
        variables: BTreeMap::new(),
        response,
        send_request: None,
    }
}

fn run_ok(script: &str, stage: ScriptStage) -> ScriptOutcome {
    run(script, &context(stage)).unwrap_or_else(|error| panic!("script failed: {error}"))
}

const STAGES: [ScriptStage; 2] = [ScriptStage::PreRequest, ScriptStage::Test];

#[test]
fn a_string_names_the_next_request_in_both_forms_and_stages() {
    for stage in STAGES {
        for form in ["pm.setNextRequest", "postman.setNextRequest"] {
            let outcome = run_ok(&format!("{form}('Login');"), stage);
            assert_eq!(
                outcome.next_request,
                Some(NextRequest::Request("Login".into())),
                "{form} in {stage}"
            );
        }
    }
}

#[test]
fn null_stops_the_run_in_both_forms_and_stages() {
    for stage in STAGES {
        for form in ["pm.setNextRequest", "postman.setNextRequest"] {
            let outcome = run_ok(&format!("{form}(null);"), stage);
            assert_eq!(
                outcome.next_request,
                Some(NextRequest::Stop),
                "{form} in {stage}"
            );
        }
    }
}

#[test]
fn the_execution_form_sets_it_too() {
    let outcome = run_ok("pm.execution.setNextRequest('id-1');", ScriptStage::Test);
    assert_eq!(
        outcome.next_request,
        Some(NextRequest::Request("id-1".into()))
    );
}

#[test]
fn the_last_call_wins() {
    let outcome = run_ok(
        "pm.setNextRequest('a'); postman.setNextRequest(null); pm.setNextRequest('b');",
        ScriptStage::Test,
    );
    assert_eq!(outcome.next_request, Some(NextRequest::Request("b".into())));
    let outcome = run_ok(
        "pm.setNextRequest('a'); postman.setNextRequest(null);",
        ScriptStage::PreRequest,
    );
    assert_eq!(outcome.next_request, Some(NextRequest::Stop));
}

#[test]
fn absent_is_none() {
    for stage in STAGES {
        let outcome = run_ok("pm.variables.set('x', '1');", stage);
        assert_eq!(outcome.next_request, None);
    }
}

#[test]
fn another_argument_type_throws() {
    for argument in ["42", "undefined", "{}", "true", ""] {
        for form in ["pm.setNextRequest", "postman.setNextRequest"] {
            let error = run(
                &format!("pm.setNextRequest('kept'); {form}({argument});"),
                &context(ScriptStage::Test),
            )
            .expect_err("a non-string, non-null argument throws");
            assert_eq!(error.kind, ScriptErrorKind::Thrown, "{form}({argument})");
            assert!(
                error.message.contains("setNextRequest"),
                "{form}({argument}): {}",
                error.message
            );
            assert_eq!(
                error.outcome.next_request,
                Some(NextRequest::Request("kept".into()))
            );
        }
    }
}
