use pretty_assertions::assert_eq;

use super::*;

const ID: &str = "01JBQ3Z7W4X9V8N2K5M6P7R8S9";

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> T {
    match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => panic!("deserialize failed: {error}\n{text}"),
    }
}

fn write<T: serde::Serialize>(value: &T) -> String {
    match toml::to_string(value) {
        Ok(text) => text,
        Err(error) => panic!("serialize failed: {error}"),
    }
}

fn request(extra: &str) -> Request {
    parse(&format!(
        "id = \"{ID}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n{extra}"
    ))
}

#[test]
fn a_document_without_description_rewrites_to_the_same_bytes() {
    let text = format!("id = \"{ID}\"\nname = \"C\"\n");
    let collection: Collection = parse(&text);
    assert_eq!(collection.description, None);
    assert_eq!(write(&collection), text);
    let folder: Folder = parse(&text);
    assert_eq!(write(&folder), text);
    let request = request("");
    assert_eq!(request.description, None);
    assert_eq!(
        write(&request),
        format!("id = \"{ID}\"\nname = \"R\"\nmethod = \"GET\"\nurl = \"u\"\n")
    );
}

#[test]
fn description_sits_right_after_the_name_and_a_multi_line_one_round_trips() {
    let text = format!(
        "id = \"{ID}\"\nname = \"C\"\ndescription = \"\"\"\n# Billing\n\nCalls the API.\n\"\"\"\nenvironments = []\n"
    );
    let collection: Collection = parse(&text);
    assert_eq!(
        collection.description.as_deref(),
        Some("# Billing\n\nCalls the API.\n")
    );
    let written = write(&collection);
    assert!(
        written.starts_with(&format!("id = \"{ID}\"\nname = \"C\"\ndescription = ")),
        "{written}"
    );
    assert!(written.lines().count() >= 4, "multi-line: {written}");
    let again: Collection = parse(&written);
    assert_eq!(again, collection);
    assert_eq!(write(&again), written);
}

#[test]
fn the_kept_postman_description_shows_when_the_field_is_empty() {
    let text = r#"{"description":{"content":"From **Postman**","type":"text/markdown"},"id":"x"}"#;
    let request = request(&format!("\n[extensions]\npostman = '{text}'\n"));
    assert_eq!(request.description, None);
    assert_eq!(
        request.description_text().as_deref(),
        Some("From **Postman**")
    );
    let plain = request_with_postman(r#"{"description":"plain"}"#);
    assert_eq!(plain.description_text().as_deref(), Some("plain"));
    assert_eq!(
        request_with_postman(r#"{"description":" "}"#).description_text(),
        None
    );
}

fn request_with_postman(json: &str) -> Request {
    request(&format!("\n[extensions]\npostman = '{json}'\n"))
}

#[test]
fn the_field_wins_over_the_kept_postman_description() {
    let mut request = request_with_postman(r#"{"description":"old"}"#);
    request.description = Some("new".to_owned());
    assert_eq!(request.description_text().as_deref(), Some("new"));
}

#[test]
fn setting_a_description_drops_only_the_kept_copy() {
    let mut request = request_with_postman(r#"{"description":"old","id":"p1"}"#);
    request.set_description("new");
    assert_eq!(request.description.as_deref(), Some("new"));
    assert_eq!(
        request
            .extensions
            .get("postman")
            .and_then(toml::Value::as_str),
        Some(r#"{"id":"p1"}"#)
    );
    request.set_description("  \n");
    assert_eq!(request.description, None);
    assert_eq!(request.description_text(), None);
}

#[test]
fn setting_a_description_removes_a_postman_text_that_held_nothing_else() {
    let mut request = request_with_postman(r#"{"description":"old"}"#);
    request.set_description("");
    assert!(request.extensions.is_empty());
    assert_eq!(request.description_text(), None);
}

#[test]
fn setting_a_description_leaves_an_unreadable_postman_text_alone() {
    let mut request = request_with_postman("not json");
    request.set_description("new");
    assert_eq!(
        request
            .extensions
            .get("postman")
            .and_then(toml::Value::as_str),
        Some("not json")
    );
}
