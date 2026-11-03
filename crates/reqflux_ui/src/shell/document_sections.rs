//! The sections of a folder's document: the one `write` emits for a request, and where
//! the sections of the document under edit are: the byte range each spans, the one a
//! cursor falls in, and which folder request each one belongs to across unsaved edits.
//!
//! A section belongs to a request by identity, not by its `###` name alone: the editor
//! starts with each section of the seed tied to its request, and every edit carries the ties
//! over — a section keeps the request of the previous section of the same name, else, when
//! it sits where a section whose name disappeared sat, that section's request. So renaming
//! a section in place keeps its request, its Retry and its response block, while moving
//! sections around still matches them by name.

use std::ops::Range;

use reqflux_core::http_document::DocumentRequest;
use reqflux_core::model::{Body, Request};

/// The request each section of the document under edit belongs to, in `parse`'s order,
/// with the names those sections had.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SectionIds {
    names: Vec<String>,
    ids: Vec<Option<String>>,
}

impl SectionIds {
    /// The ties of a document whose sections are `sections`, `(request id, name)` in order.
    pub(super) fn new<'a>(sections: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let (ids, names) = sections
            .into_iter()
            .map(|(id, name)| (Some(id.to_owned()), name.to_owned()))
            .unzip();
        Self { names, ids }
    }

    /// Carries the ties over to a document whose sections are now named `names`, in order.
    pub(super) fn track(&mut self, names: Vec<String>) {
        let mut taken = vec![false; self.ids.len()];
        let mut ids: Vec<Option<String>> = vec![None; names.len()];
        for (index, name) in names.iter().enumerate() {
            let found = self
                .names
                .iter()
                .enumerate()
                .find(|(at, old)| *old == name && !taken.get(*at).copied().unwrap_or(true));
            if let Some((at, _)) = found {
                if let Some(slot) = taken.get_mut(at) {
                    *slot = true;
                }
                if let Some(id) = ids.get_mut(index) {
                    *id = self.ids.get(at).cloned().flatten();
                }
            }
        }
        for (index, name) in names.iter().enumerate() {
            let renamed = ids.get(index).is_some_and(Option::is_none)
                && !taken.get(index).copied().unwrap_or(true)
                && self
                    .names
                    .get(index)
                    .is_some_and(|old| old != name && !names.contains(old));
            if renamed {
                if let Some(slot) = taken.get_mut(index) {
                    *slot = true;
                }
                if let Some(id) = ids.get_mut(index) {
                    *id = self.ids.get(index).cloned().flatten();
                }
            }
        }
        self.names = names;
        self.ids = ids;
    }

    /// The request the section `index` belongs to, if any.
    pub(super) fn id(&self, index: usize) -> Option<&str> {
        self.ids.get(index)?.as_deref()
    }

    /// The index of the section that belongs to the request `id`, if one still does.
    pub(super) fn index_of(&self, id: &str) -> Option<usize> {
        self.ids
            .iter()
            .position(|candidate| candidate.as_deref() == Some(id))
    }
}

/// The section of `text` the cursor `offset` falls in, as its index in `parse`'s order:
/// the last one that starts before the cursor when it sits between sections; `None`
/// before the first one.
pub(super) fn section_at(text: &str, offset: usize) -> Option<usize> {
    section_ranges(text)
        .iter()
        .rposition(|range| range.start <= offset)
}

/// The byte range each section of `text` spans, `parse`'s sections in order: from the
/// separator, or the first content line of the unnamed first section, to where the next
/// section starts — the end of the text for the last one. A `###` line opens a section
/// under the same rule `parse` applies.
fn section_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut start: Option<usize> = None;
    let mut in_body = false;
    let mut previous_blank = false;
    let mut offset = 0_usize;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let content = line.trim_end();
        let blank = content.trim().is_empty();
        if content.starts_with("###") && (!in_body || previous_blank) {
            if let Some(from) = start.take() {
                ranges.push(from..line_start);
            }
            start = Some(line_start);
            in_body = false;
            previous_blank = false;
            continue;
        }
        match start {
            None => {
                if !blank && !is_head_comment(content) {
                    start = Some(line_start);
                }
            }
            Some(_) if blank && !in_body => in_body = true,
            Some(_) => {}
        }
        previous_blank = blank;
    }
    if let Some(from) = start {
        ranges.push(from..offset);
    }
    ranges
}

/// A head comment line, the ones `parse` ignores: a trimmed `#` or `//` line.
fn is_head_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') || trimmed.starts_with("//")
}

/// The section `write` emits for `request`: the name, the method, the URL with the
/// enabled params appended after the separator `?` or `&` the URL asks for, the enabled
/// headers, and the raw body; the other body kinds have no `.http` text yet.
pub(super) fn document_section(request: &Request) -> DocumentRequest {
    DocumentRequest {
        name: request.name.clone(),
        method: request.method.clone(),
        url: full_url(request),
        headers: request
            .headers
            .iter()
            .filter(|header| header.enabled)
            .cloned()
            .collect(),
        body: match &request.body {
            Body::Raw { text, .. } => text.clone(),
            Body::None
            | Body::Urlencoded { .. }
            | Body::FormData { .. }
            | Body::Binary { .. }
            | Body::Graphql { .. } => String::new(),
        },
    }
}

/// The URL of `request` as written, its enabled params appended after `?`, or `&` when
/// the URL already carries a query.
fn full_url(request: &Request) -> String {
    let params: Vec<String> = request
        .params
        .iter()
        .filter(|param| param.enabled)
        .map(|param| format!("{}={}", param.key, param.value))
        .collect();
    if params.is_empty() {
        return request.url.clone();
    }
    let separator = if request.url.contains('?') { "&" } else { "?" };
    format!("{}{separator}{}", request.url, params.join("&"))
}

#[cfg(test)]
mod tests {
    use super::SectionIds;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn ids() -> SectionIds {
        SectionIds::new([("a", "Create"), ("b", "List")])
    }

    #[test]
    fn a_rename_in_place_keeps_the_request() {
        let mut tracked = ids();
        tracked.track(names(&["Create draft", "List"]));
        assert_eq!(tracked.id(0), Some("a"));
        assert_eq!(tracked.id(1), Some("b"));
    }

    #[test]
    fn moved_sections_match_by_name() {
        let mut tracked = ids();
        tracked.track(names(&["List", "Create"]));
        assert_eq!(tracked.id(0), Some("b"));
        assert_eq!(tracked.id(1), Some("a"));
    }

    #[test]
    fn a_new_section_belongs_to_no_request() {
        let mut tracked = ids();
        tracked.track(names(&["Health", "Create", "List"]));
        assert_eq!(tracked.id(0), None);
        assert_eq!(tracked.index_of("a"), Some(1));
        assert_eq!(tracked.index_of("b"), Some(2));
    }

    #[test]
    fn a_deleted_section_leaves_its_request_without_one() {
        let mut tracked = ids();
        tracked.track(names(&["List"]));
        assert_eq!(tracked.index_of("a"), None);
        assert_eq!(tracked.index_of("b"), Some(0));
    }
}
