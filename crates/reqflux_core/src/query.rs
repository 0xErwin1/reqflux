//! The query string of a request URL as written, and the params it maps to.
//!
//! A request keeps its URL without the query and its params as rows; front ends show the
//! URL with the params that are sent appended as written, and a typed query becomes rows
//! again. Nothing here encodes or decodes: `{{variables}}` and the order survive both ways.
//!
//! A row has no flag for "typed with `=`": a row with an empty value is written as its bare
//! key, so `?b` round-trips as `b` and a typed `?b=` shows as `b` once it is a row. The wire
//! form the engine sends keeps `key=` (see [`append_query`]).

use crate::model::KeyValue;

/// Whether `row` takes part in the query: it is enabled and has a key or a value. A row with
/// an empty key and an empty value is never sent nor shown.
#[must_use]
pub fn is_sent(row: &KeyValue) -> bool {
    row.enabled && !(row.key.is_empty() && row.value.is_empty())
}

/// Splits `url` into the URL without its query and one enabled row per pair of the query, in
/// order. A pair without `=` has an empty value; empty pairs (`&&`, a trailing `&`) are
/// skipped; repeated keys stay separate rows. A `#fragment` stays on the returned URL.
#[must_use]
pub fn split(url: &str) -> (String, Vec<KeyValue>) {
    let (before, fragment) = without_fragment(url);
    let Some((path, query)) = before.split_once('?') else {
        return (url.to_owned(), Vec::new());
    };
    let rows = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            KeyValue::new(key, value)
        })
        .collect();
    (format!("{path}{fragment}"), rows)
}

/// `base` with the rows of `params` that are sent appended as written; see [`append`].
#[must_use]
pub fn compose(base: &str, params: &[KeyValue]) -> String {
    let pairs: Vec<(String, String)> = params
        .iter()
        .filter(|row| is_sent(row))
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect();
    append(base.to_owned(), &pairs)
}

/// Appends `pairs` to `base` as written, as `key=value` joined by `&` (a pair with an empty
/// value is its bare `key`), with the separator of [`append_query`].
#[must_use]
pub fn append(base: String, pairs: &[(String, String)]) -> String {
    let query: Vec<String> = pairs
        .iter()
        .map(|(key, value)| {
            if value.is_empty() {
                key.clone()
            } else {
                format!("{key}={value}")
            }
        })
        .collect();
    append_query(base, &query)
}

/// Appends the already written `pairs` to `base` joined by `&`, after `&` when `base`
/// already has a query and `?` when it does not, and before a `#fragment`.
#[must_use]
pub fn append_query(base: String, pairs: &[String]) -> String {
    if pairs.is_empty() {
        return base;
    }
    let (before, fragment) = without_fragment(&base);
    let separator = if before.contains('?') { '&' } else { '?' };
    format!("{before}{separator}{}{fragment}", pairs.join("&"))
}

/// The params of a request whose URL now carries `query`: every row of `rows` that is sent
/// is replaced, in order, by the next row of `query`, keeping its description when the key
/// is the same; rows that are not sent (disabled or blank) keep their place; sent rows left
/// over are dropped and rows of `query` left over are appended.
#[must_use]
pub fn merge(rows: &[KeyValue], query: Vec<KeyValue>) -> Vec<KeyValue> {
    let mut query = query.into_iter();
    let mut merged = Vec::with_capacity(rows.len());
    for row in rows {
        if !is_sent(row) {
            merged.push(row.clone());
        } else if let Some(mut next) = query.next() {
            if next.key == row.key {
                next.description.clone_from(&row.description);
            }
            merged.push(next);
        }
    }
    merged.extend(query);
    merged
}

/// `url` before its `#`, and the `#fragment` itself, empty when there is none.
fn without_fragment(url: &str) -> (&str, &str) {
    match url.find('#') {
        Some(at) => url.split_at(at),
        None => (url, ""),
    }
}
