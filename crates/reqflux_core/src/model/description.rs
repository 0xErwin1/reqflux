//! The markdown description of a collection, folder or request.
//!
//! Collections imported before `description` existed kept the Postman text inside
//! `extensions.postman`. Reading falls back to that copy and never rewrites the file;
//! setting a description writes the field and drops the copy, so clearing the text cannot
//! bring the old one back.

use serde_json::Value;

use super::{Collection, Folder, Request};

const POSTMAN: &str = "postman";
const KEY: &str = "description";

/// The description Postman exported on a collection, item or request: a string or
/// `{content}`. Blank text is no description.
#[must_use]
pub fn postman_description(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(text) => text.as_str(),
        Value::Object(object) => object.get("content")?.as_str()?,
        _ => return None,
    };
    (!text.trim().is_empty()).then(|| text.to_owned())
}

/// The description kept by an import in `extensions.postman`, when there is one.
fn legacy(extensions: &toml::Table) -> Option<String> {
    let text = extensions.get(POSTMAN)?.as_str()?;
    let rest: Value = serde_json::from_str(text).ok()?;
    postman_description(rest.get(KEY))
}

/// Drops the `description` key of the kept Postman text, and the text itself when nothing
/// else is left in it. Leaves an unreadable text alone.
fn drop_legacy(extensions: &mut toml::Table) {
    let Some(text) = extensions.get(POSTMAN).and_then(toml::Value::as_str) else {
        return;
    };
    let Ok(Value::Object(mut rest)) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if rest.remove(KEY).is_none() {
        return;
    }
    if rest.is_empty() {
        extensions.remove(POSTMAN);
    } else if let Ok(text) = serde_json::to_string(&Value::Object(rest)) {
        extensions.insert(POSTMAN.to_owned(), toml::Value::String(text));
    }
}

fn effective(own: Option<&String>, extensions: &toml::Table) -> Option<String> {
    own.cloned().or_else(|| legacy(extensions))
}

fn set(own: &mut Option<String>, extensions: &mut toml::Table, text: &str) {
    *own = (!text.trim().is_empty()).then(|| text.to_owned());
    drop_legacy(extensions);
}

macro_rules! describable {
    ($($document:ty),*) => {$(
        impl $document {
            /// The description to show: the field, else the text an earlier Postman import
            /// kept in `extensions.postman`.
            #[must_use]
            pub fn description_text(&self) -> Option<String> {
                effective(self.description.as_ref(), &self.extensions)
            }

            /// Sets the description; blank text clears it. The copy in `extensions.postman`
            /// goes away, so the edit is the only description left.
            pub fn set_description(&mut self, text: &str) {
                set(&mut self.description, &mut self.extensions, text);
            }
        }
    )*};
}

describable!(Collection, Folder, Request);
