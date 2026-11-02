//! Tests for the `.http` document parser and writer (v1-features S7).

use pretty_assertions::assert_eq;

use crate::http_document::{self, DocumentRequest};
use crate::model::{Body, Id, KeyValue, Method, RawLanguage};

/// The document shape of design/layouts/02-document-layout.html, main frame: four
/// sections, headers, one JSON body, and one blank line between sections.
const ARTBOARD: &str = r#"### List invoices
GET {{base_url}}/v1/invoices?expand=lines
Authorization: Bearer ********
Accept: application/json

### Create invoice
POST {{base_url}}/v1/invoices
Content-Type: application/json
Authorization: Bearer ********
X-Tenant-Id: {{tenant_id}}
Idempotency-Key: {{$guid}}

{
  "customer": "cus_9s6XKzkNRiz8i3",
  "currency": "eur"
}

### Get invoice
GET {{base_url}}/v1/invoices/{{id}}
Authorization: Bearer ********

### Void invoice
PATCH {{base_url}}/v1/invoices/{{id}}/void
Authorization: Bearer ********
"#;

fn header(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

// --- parse: the artboard shape ----------------------------------------------

#[test]
fn parses_the_artboard_document() {
    let (requests, warnings) = http_document::parse(ARTBOARD);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 4);

    assert_eq!(requests[0].name, "List invoices");
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].url, "{{base_url}}/v1/invoices?expand=lines");
    assert_eq!(
        requests[0].headers,
        vec![
            header("Authorization", "Bearer ********"),
            header("Accept", "application/json"),
        ]
    );
    assert_eq!(requests[0].body, "");

    assert_eq!(requests[1].name, "Create invoice");
    assert_eq!(requests[1].method, Method::POST);
    assert_eq!(requests[1].url, "{{base_url}}/v1/invoices");
    assert_eq!(
        requests[1].headers,
        vec![
            header("Content-Type", "application/json"),
            header("Authorization", "Bearer ********"),
            header("X-Tenant-Id", "{{tenant_id}}"),
            header("Idempotency-Key", "{{$guid}}"),
        ]
    );
    assert_eq!(
        requests[1].body,
        "{\n  \"customer\": \"cus_9s6XKzkNRiz8i3\",\n  \"currency\": \"eur\"\n}"
    );

    assert_eq!(requests[2].name, "Get invoice");
    assert_eq!(requests[2].method, Method::GET);
    assert_eq!(requests[2].url, "{{base_url}}/v1/invoices/{{id}}");
    assert_eq!(
        requests[2].headers,
        vec![header("Authorization", "Bearer ********")]
    );
    assert_eq!(requests[2].body, "");

    assert_eq!(requests[3].name, "Void invoice");
    assert_eq!(requests[3].method, Method::PATCH);
    assert_eq!(requests[3].url, "{{base_url}}/v1/invoices/{{id}}/void");
    assert_eq!(
        requests[3].headers,
        vec![header("Authorization", "Bearer ********")]
    );
    assert_eq!(requests[3].body, "");
}

// --- write and round trips ---------------------------------------------------

#[test]
fn writes_the_artboard_back_byte_for_byte() {
    let (requests, _) = http_document::parse(ARTBOARD);
    assert_eq!(http_document::write(&requests), ARTBOARD);
}

#[test]
fn round_trips_parse_write_parse() {
    let (requests, _) = http_document::parse(ARTBOARD);
    let text = http_document::write(&requests);
    let (reparsed, warnings) = http_document::parse(&text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(reparsed, requests);
}

#[test]
fn round_trips_one_canonical_section_byte_for_byte() {
    let text =
        "### Solo\nPOST https://api.example.com/v1\nContent-Type: application/json\n\n{\"a\": 1}\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 1);
    assert_eq!(http_document::write(&requests), text);
}

#[test]
fn writes_a_body_with_trailing_blank_lines_verbatim() {
    let text = "### A\nGET https://example.com\n\nbody\n\n";
    let (requests, _) = http_document::parse(text);
    assert_eq!(requests[0].body, "body\n");
    assert_eq!(http_document::write(&requests), text);
}

// --- the separator rule -------------------------------------------------------

/// Chosen rule (module doc): a `###` line at column zero after a blank line, or any
/// `###` line before the body begins, opens a section. An indented `###` line inside a
/// body, or one that directly follows body text, is body text.
#[test]
fn indented_separator_inside_a_body_does_not_split() {
    let text = "### A\nPOST https://example.com\n\nline1\n  ### not a section\nline3\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, "line1\n  ### not a section\nline3");
}

#[test]
fn separator_without_a_blank_line_inside_a_body_does_not_split() {
    let text = "### A\nPOST https://example.com\n\nline1\n### trap\nline3\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, "line1\n### trap\nline3");
}

#[test]
fn separator_after_a_blank_line_inside_a_body_opens_a_section() {
    let text = "### A\nPOST https://example.com\n\n{\"a\": 1}\n\n### B\nGET https://example.com\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, "{\"a\": 1}");
    assert_eq!(requests[1].name, "B");
}

// --- first section, comments and the method line ------------------------------

#[test]
fn first_section_needs_no_separator_and_is_unnamed() {
    let text = "GET https://api.example.com/ping\nX-Trace: 1\n\nhello\nworld\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].name, "");
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].url, "https://api.example.com/ping");
    assert_eq!(requests[0].headers, vec![header("X-Trace", "1")]);
    assert_eq!(requests[0].body, "hello\nworld");
}

#[test]
fn comments_are_ignored_and_not_written_back() {
    let text = "# top comment\n// another\n### With comments\n# explain\nGET https://example.com\nX: 1\n\n### Second\nPOST https://example.com\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].name, "With comments");
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].headers, vec![header("X", "1")]);
    assert_eq!(requests[1].name, "Second");
    let written = http_document::write(&requests);
    assert!(!written.contains("top comment"));
    assert!(!written.contains("explain"));
    assert!(!written.contains("another"));
    assert_eq!(http_document::parse(&written).0, requests);
}

#[test]
fn a_bare_url_line_defaults_to_get_and_methods_are_uppercased() {
    let (requests, warnings) = http_document::parse("https://example.com/x\n");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].url, "https://example.com/x");

    let (requests, _) = http_document::parse("### A\nget https://example.com\n");
    assert_eq!(requests[0].method, Method::GET);
}

#[test]
fn a_header_without_a_space_after_the_colon_is_still_a_header() {
    let (requests, warnings) =
        http_document::parse("### A\nGET https://example.com\nContent-Type:application/json\n");
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(
        requests[0].headers,
        vec![header("Content-Type", "application/json")]
    );
}

// --- warnings -----------------------------------------------------------------

#[test]
fn a_section_without_a_method_line_warns_and_is_skipped() {
    let text = "### Empty\n\n### Real\nGET https://example.com\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].name, "Real");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("Empty"), "warning: {}", warnings[0]);
    assert!(
        warnings[0].contains("no method or URL line"),
        "warning: {}",
        warnings[0]
    );
}

#[test]
fn a_section_that_opens_with_a_header_warns_and_is_skipped() {
    let text = "### Headers only\nA: 1\nB: 2\n";
    let (requests, warnings) = http_document::parse(text);
    assert!(requests.is_empty());
    assert_eq!(
        warnings.len(),
        1,
        "one warning, not one per line: {warnings:?}"
    );
    assert!(warnings[0].contains("Headers only"));
}

#[test]
fn a_head_line_without_a_colon_warns_and_is_ignored() {
    let text = "### A\nGET https://example.com\nnot a header\n";
    let (requests, warnings) = http_document::parse(text);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].headers, Vec::<KeyValue>::new());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("line 3"), "warning: {}", warnings[0]);
}

#[test]
fn an_empty_document_yields_nothing() {
    assert_eq!(http_document::parse(""), (Vec::new(), Vec::new()));
    assert_eq!(
        http_document::parse("# only comments\n\n// nothing\n").0,
        Vec::<DocumentRequest>::new()
    );
}

// --- to_request ----------------------------------------------------------------

#[test]
fn to_request_splits_the_query_into_params_and_maps_the_body() {
    let section = DocumentRequest {
        name: "List invoices".to_owned(),
        method: Method::GET,
        url: "{{base_url}}/v1/invoices?expand=lines&page={{page}}".to_owned(),
        headers: vec![header("Content-Type", "application/json")],
        body: "{\"a\": 1}".to_owned(),
    };
    let request = section.to_request(Id::new());
    assert_eq!(request.name, "List invoices");
    assert_eq!(request.method, Method::GET);
    assert_eq!(request.url, "{{base_url}}/v1/invoices");
    assert_eq!(
        request.params,
        vec![header("expand", "lines"), header("page", "{{page}}")]
    );
    assert_eq!(
        request.headers,
        vec![header("Content-Type", "application/json")]
    );
    assert_eq!(
        request.body,
        Body::Raw {
            language: RawLanguage::Json,
            text: "{\"a\": 1}".to_owned()
        }
    );
}

#[test]
fn to_request_keeps_a_query_mark_inside_a_template() {
    let section = DocumentRequest {
        name: "A".to_owned(),
        method: Method::GET,
        url: "{{redirect?url}}/x?q={{term}}".to_owned(),
        headers: Vec::new(),
        body: String::new(),
    };
    let request = section.to_request(Id::new());
    assert_eq!(request.url, "{{redirect?url}}/x");
    assert_eq!(request.params, vec![header("q", "{{term}}")]);
}

#[test]
fn to_request_without_body_maps_to_body_none_and_inherits_auth() {
    let section = DocumentRequest {
        name: "Ping".to_owned(),
        method: Method::DELETE,
        url: "https://example.com".to_owned(),
        headers: vec![header("X", "1")],
        body: String::new(),
    };
    let request = section.to_request(Id::new());
    assert_eq!(request.url, "https://example.com");
    assert!(request.params.is_empty());
    assert_eq!(request.body, Body::None);
    assert_eq!(request.auth, crate::model::Auth::Inherit);
    assert_eq!(request.headers, vec![header("X", "1")]);
}

#[test]
fn to_request_maps_a_plain_text_body_to_text() {
    let section = DocumentRequest {
        name: "A".to_owned(),
        method: Method::POST,
        url: "https://example.com".to_owned(),
        headers: Vec::new(),
        body: "hello".to_owned(),
    };
    let request = section.to_request(Id::new());
    assert_eq!(
        request.body,
        Body::Raw {
            language: RawLanguage::Text,
            text: "hello".to_owned()
        }
    );
}
