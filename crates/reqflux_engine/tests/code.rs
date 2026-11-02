//! `code`: a stored request as code, with its effective auth and nothing resolved
//! (v1-features S2).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test helpers fail the calling test"
)]

mod common;

use common::{collection, folder, get, request, single, workspace};
use pretty_assertions::assert_eq;
use reqflux_core::codegen::Language;
use reqflux_core::model::{Auth, KeyValue, Variable};

#[test]
fn code_uses_the_inherited_auth_and_leaves_variables_and_secrets_unresolved() {
    let mut doc = get("{{base}}/me");
    doc.headers.push(KeyValue::new("X-Trace", "{{trace}}"));
    let mut tree = workspace(
        vec![
            Variable::new("base", "https://api.example.com"),
            Variable {
                secret: true,
                ..Variable::new("token", "s3cret")
            },
        ],
        vec![collection(
            "c",
            vec![folder(
                "f",
                Auth::Bearer {
                    token: "{{token}}".to_owned(),
                },
                vec![request("r", doc)],
            )],
        )],
    );
    tree.collections[0].collection.auth = Auth::None;
    let code = match reqflux_engine::code(&tree, "c/f/r", Language::Curl) {
        Ok(code) => code,
        Err(error) => panic!("code: {error}"),
    };
    assert_eq!(
        code,
        "curl '{{base}}/me' \\\n  -H 'X-Trace: {{trace}}' \\\n  --oauth2-bearer '{{token}}'"
    );
    assert!(!code.contains("s3cret"));
}

#[test]
fn code_of_an_unknown_request_is_a_usage_error() {
    let tree = single(get("https://x.io"));
    let error = match reqflux_engine::code(&tree, "c/nope", Language::Python) {
        Ok(code) => panic!("unexpected code {code}"),
        Err(error) => error,
    };
    assert!(error.is_usage());
    assert_eq!(error.to_string(), "no request matches c/nope");
}
