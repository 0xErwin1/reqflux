//! The query string of a URL as written and the params it maps to.

use reqflux_core::model::KeyValue;
use reqflux_core::query::{append, compose, is_sent, merge, split};

fn row(key: &str, value: &str) -> KeyValue {
    KeyValue::new(key, value)
}

fn disabled(key: &str, value: &str) -> KeyValue {
    KeyValue {
        enabled: false,
        ..KeyValue::new(key, value)
    }
}

fn described(key: &str, value: &str, description: &str) -> KeyValue {
    KeyValue {
        description: Some(description.to_owned()),
        ..KeyValue::new(key, value)
    }
}

#[test]
fn split_moves_the_query_into_rows_in_order() {
    assert_eq!(
        split("https://h/p?test=test&b=2"),
        (
            "https://h/p".to_owned(),
            vec![row("test", "test"), row("b", "2")]
        )
    );
}

#[test]
fn split_keeps_variables_as_written() {
    assert_eq!(
        split("{{base}}/v1?token={{token}}&{{name}}=x"),
        (
            "{{base}}/v1".to_owned(),
            vec![row("token", "{{token}}"), row("{{name}}", "x")]
        )
    );
}

#[test]
fn split_keeps_empty_values_and_skips_empty_pairs() {
    assert_eq!(
        split("http://h?a=&b&&=v&"),
        (
            "http://h".to_owned(),
            vec![row("a", ""), row("b", ""), row("", "v")]
        )
    );
}

#[test]
fn split_keeps_repeated_keys_as_separate_rows() {
    assert_eq!(
        split("http://h?id=1&id=2"),
        ("http://h".to_owned(), vec![row("id", "1"), row("id", "2")])
    );
}

#[test]
fn split_keeps_the_value_after_the_first_equals_sign() {
    assert_eq!(
        split("http://h?q=a=b"),
        ("http://h".to_owned(), vec![row("q", "a=b")])
    );
}

#[test]
fn split_leaves_the_fragment_on_the_url() {
    assert_eq!(
        split("http://h/p?a=1#top"),
        ("http://h/p#top".to_owned(), vec![row("a", "1")])
    );
    assert_eq!(
        split("http://h/p#a?b=1"),
        ("http://h/p#a?b=1".to_owned(), vec![]),
        "a `?` after the `#` is part of the fragment"
    );
}

#[test]
fn split_of_a_url_without_query_is_the_url_and_no_rows() {
    assert_eq!(split("http://h/p"), ("http://h/p".to_owned(), vec![]));
    assert_eq!(split("http://h/p?"), ("http://h/p".to_owned(), vec![]));
    assert_eq!(split(""), (String::new(), vec![]));
}

#[test]
fn compose_appends_the_sent_rows_as_written() {
    let params = [
        row("a", "{{x}}"),
        disabled("off", "1"),
        row("", ""),
        row("b", ""),
        row("id", "1"),
        row("id", "2"),
    ];
    assert_eq!(
        compose("{{base}}/p", &params),
        "{{base}}/p?a={{x}}&b&id=1&id=2"
    );
}

#[test]
fn compose_without_sent_rows_is_the_base() {
    assert_eq!(compose("http://h", &[]), "http://h");
    assert_eq!(
        compose("http://h", &[row("", ""), disabled("a", "1")]),
        "http://h"
    );
}

#[test]
fn compose_puts_the_query_before_the_fragment() {
    assert_eq!(
        compose("http://h/p#top", &[row("a", "1")]),
        "http://h/p?a=1#top"
    );
}

#[test]
fn append_after_an_existing_query_uses_an_ampersand() {
    let pairs = [("b".to_owned(), "2".to_owned())];
    assert_eq!(
        append("http://h?a=1".to_owned(), &pairs),
        "http://h?a=1&b=2"
    );
    assert_eq!(
        append("http://h?a=1#f".to_owned(), &pairs),
        "http://h?a=1&b=2#f"
    );
    assert_eq!(append("http://h".to_owned(), &[]), "http://h");
}

#[test]
fn a_pair_without_a_value_is_written_without_an_equals_sign() {
    assert_eq!(compose("http://h", &[row("b", "")]), "http://h?b");
    let pairs = [
        ("b".to_owned(), String::new()),
        (String::new(), "v".to_owned()),
    ];
    assert_eq!(append("http://h".to_owned(), &pairs), "http://h?b&=v");
}

#[test]
fn a_typed_key_without_a_value_round_trips_as_typed() {
    let (base, rows) = split("http://h?a=1&b");
    assert_eq!(compose(&base, &rows), "http://h?a=1&b");
}

#[test]
fn compose_then_split_round_trips_the_sent_rows() {
    let params = vec![row("a", "{{x}}"), row("b", ""), row("", "v"), row("a", "2")];
    let url = compose("http://h/p#f", &params);
    assert_eq!(split(&url), ("http://h/p#f".to_owned(), params));
}

#[test]
fn is_sent_needs_an_enabled_row_with_a_key_or_a_value() {
    assert!(is_sent(&row("a", "")));
    assert!(is_sent(&row("", "v")));
    assert!(!is_sent(&row("", "")));
    assert!(!is_sent(&disabled("a", "1")));
}

#[test]
fn merge_replaces_the_sent_rows_and_keeps_disabled_ones_in_place() {
    let rows = [row("a", "1"), disabled("off", "x"), row("b", "2")];
    let merged = merge(&rows, vec![row("a", "9"), row("c", "3")]);
    assert_eq!(
        merged,
        vec![row("a", "9"), disabled("off", "x"), row("c", "3")]
    );
}

#[test]
fn merge_keeps_blank_rows_and_the_description_of_the_same_key() {
    let rows = [
        described("a", "1", "the a"),
        row("", ""),
        described("b", "2", "the b"),
    ];
    let merged = merge(&rows, vec![row("a", "5"), row("z", "2")]);
    assert_eq!(
        merged,
        vec![described("a", "5", "the a"), row("", ""), row("z", "2")]
    );
}

#[test]
fn merge_drops_sent_rows_the_query_no_longer_has_and_appends_new_ones() {
    let rows = [row("a", "1"), row("b", "2"), disabled("off", "x")];
    assert_eq!(merge(&rows, vec![]), vec![disabled("off", "x")]);
    assert_eq!(
        merge(&[disabled("off", "x")], vec![row("test", "test")]),
        vec![disabled("off", "x"), row("test", "test")]
    );
}
