//! The texts and values of the example views (v1-features S4).

use std::path::Path;

use reqflux_core::model::{
    Example, ExampleBody, ExampleRequest, ExampleResponse, Id, KeyValue, Method,
};
use reqflux_engine::{MASK, Response, SavedExample, Timings};

use super::{
    ShownBody, body_file, body_saved_subtitle, content_type, default_name, masked_count,
    saved_response, saved_subtitle, saved_title, shown_body, status_line, suggested_file_name,
};

fn response(body: &[u8], headers: &[(&str, &str)]) -> Response {
    Response {
        status: 401,
        reason: Some("Unauthorized".to_owned()),
        version: "1.1".to_owned(),
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: body.to_vec(),
        final_url: String::new(),
        elapsed: std::time::Duration::ZERO,
        timings: Timings::default(),
    }
}

fn saved(body: ExampleBody, bytes: &[u8]) -> SavedExample {
    SavedExample {
        slug: "ok".to_owned(),
        example: Example {
            id: Id::new(),
            name: "OK".to_owned(),
            request: ExampleRequest {
                method: Method::GET,
                url: "{{base}}/v1".to_owned(),
                params: Vec::new(),
                headers: Vec::new(),
                auth: reqflux_core::model::Auth::default(),
                body: reqflux_core::model::Body::None,
            },
            response: ExampleResponse {
                status: 200,
                reason: Some("OK".to_owned()),
                headers: vec![KeyValue::new("content-type", "image/png")],
                body,
            },
            extensions: toml::Table::new(),
        },
        body: bytes.to_vec(),
    }
}

#[test]
fn a_new_example_is_named_after_the_reason_phrase() {
    assert_eq!(default_name(401, Some("Unauthorized")), "Unauthorized");
    assert_eq!(default_name(299, None), "299");
    assert_eq!(default_name(299, Some("  ")), "299");
    assert_eq!(status_line(401, Some("Unauthorized")), "401 Unauthorized");
    assert_eq!(status_line(299, None), "299");
}

#[test]
fn the_masked_count_counts_each_hidden_value_once() {
    let response = response(
        br#"{"token":"tok-123","again":"tok-123"}"#,
        &[("x-token", "tok-123"), ("content-type", "application/json")],
    );
    let mask = |text: &str| text.replace("tok-123", MASK);
    assert_eq!(masked_count(&response, mask), 3);
    assert_eq!(masked_count(&response, str::to_owned), 0);
}

#[test]
fn the_saved_toast_names_the_example_and_the_secrets() {
    assert_eq!(saved_title("Unauthorized"), "Saved example Unauthorized");
    assert_eq!(
        saved_subtitle("List invoices", 1),
        "List invoices \u{b7} 1 secret masked in the response"
    );
    assert_eq!(
        saved_subtitle("List invoices", 2),
        "List invoices \u{b7} 2 secrets masked in the response"
    );
    assert_eq!(saved_subtitle("List invoices", 0), "List invoices");
    assert_eq!(
        body_saved_subtitle(Path::new("/tmp/invoices.json"), 1229),
        "/tmp/invoices.json \u{b7} 1.2 KB"
    );
}

#[test]
fn the_save_dialog_suggests_a_name_from_the_content_type() {
    assert_eq!(
        suggested_file_name("List invoices", "application/json"),
        "list-invoices.json"
    );
    assert_eq!(suggested_file_name("Logo", "image/png"), "logo.png");
    assert_eq!(suggested_file_name("x", "text/plain"), "x.txt");
    assert_eq!(suggested_file_name("", "application/zip"), "response.bin");
    assert_eq!(
        content_type([("Content-Type", "application/json; charset=utf-8")]),
        "application/json"
    );
}

#[test]
fn a_text_body_marks_the_lines_with_a_masked_value() {
    let text = format!("{{\n  \"token\": \"{MASK}\"\n}}");
    let shown = shown_body(&saved(ExampleBody::Text(text.clone()), text.as_bytes()));
    assert_eq!(
        shown,
        ShownBody::Text(vec![
            ("{".to_owned(), false),
            (format!("  \"token\": \"{MASK}\""), true),
            ("}".to_owned(), false),
        ])
    );
    assert_eq!(shown_body(&saved(ExampleBody::None, b"")), ShownBody::Empty);
}

#[test]
fn a_binary_body_shows_its_file_type_and_size_and_saves_as_received() {
    let bytes = [0x89, 0x50, 0x4e, 0x47, 0xff];
    let example = saved(ExampleBody::File("ok.body".to_owned()), &bytes);
    assert_eq!(
        shown_body(&example),
        ShownBody::Binary {
            file: "ok.body".to_owned(),
            content_type: "image/png".to_owned(),
            bytes: 5,
        }
    );
    let response = saved_response(&example);
    assert_eq!(response.body, bytes);
    assert_eq!(response.status, 200);
    assert_eq!(
        body_file("preview.examples/ok.example.toml", "ok.body"),
        "preview.examples/ok.body"
    );
}
