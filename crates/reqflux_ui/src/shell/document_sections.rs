//! The sections of a folder's document: the one `write` emits for a request, and where
//! the sections of the document under edit are: the byte range each spans, the one a
//! cursor falls in, and which folder request each one belongs to across unsaved edits.
//!
//! A section belongs to a request only while it is unambiguously the same section: the
//! editor starts with each section of the seed tied to its request, and every edit carries
//! the ties over — a section keeps the request of the previous section of the same name,
//! else of the seed section of that name; failing both, it keeps the request of the section
//! it replaces only in a rename in place, where the count of sections is unchanged, every
//! section that kept its name kept its place, the section kept its request line (method and
//! URL), and the new name is not two headers glued together (`New### List` while a header
//! is typed above another). A new, merged or
//! otherwise ambiguous section belongs to no request, so it can never send as another one.

use std::ops::Range;

use reqflux_core::http_document::{self, DocumentRequest};
use reqflux_core::model::{Body, Request};

/// The request each section of the document under edit belongs to, in `parse`'s order,
/// with the names and request lines those sections had, and the name each request had in
/// the seed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SectionIds {
    names: Vec<String>,
    lines: Vec<String>,
    ids: Vec<Option<String>>,
    seed: Vec<(String, String)>,
}

impl SectionIds {
    /// The ties of a document whose sections are `sections`, `(request id, section)` in
    /// order.
    pub(super) fn new<'a>(
        sections: impl IntoIterator<Item = (&'a str, &'a DocumentRequest)>,
    ) -> Self {
        let mut ties = Self::default();
        for (id, section) in sections {
            ties.names.push(section.name.clone());
            ties.lines.push(request_line(section));
            ties.ids.push(Some(id.to_owned()));
            ties.seed.push((id.to_owned(), section.name.clone()));
        }
        ties
    }

    /// Carries the ties over to a document whose sections are now `sections`, in order.
    pub(super) fn track(&mut self, sections: &[DocumentRequest]) {
        let names: Vec<String> = sections
            .iter()
            .map(|section| section.name.clone())
            .collect();
        let lines: Vec<String> = sections.iter().map(request_line).collect();
        let mut ids: Vec<Option<String>> = vec![None; names.len()];
        let mut taken = vec![false; self.ids.len()];
        let mut in_place = names.len() == self.names.len();
        for (index, name) in names.iter().enumerate() {
            let found = (0..self.names.len()).find(|at| {
                self.names.get(*at) == Some(name)
                    && !taken.get(*at).copied().unwrap_or(true)
                    && self.ids.get(*at).is_some_and(Option::is_some)
            });
            if let Some(at) = found {
                in_place &= at == index;
                if let Some(slot) = taken.get_mut(at) {
                    *slot = true;
                }
                if let Some(id) = ids.get_mut(index) {
                    *id = self.ids.get(at).cloned().flatten();
                }
            }
        }
        for index in 0..names.len() {
            if ids.get(index).is_some_and(Option::is_some) {
                continue;
            }
            let Some(name) = names.get(index) else {
                continue;
            };
            let seeded = self
                .seed
                .iter()
                .find(|(id, seed_name)| {
                    seed_name == name && !ids.iter().flatten().any(|used| used == id)
                })
                .map(|(id, _)| id.clone());
            if let Some(id) = seeded {
                if let Some(at) = self.ids.iter().position(|old| old.as_ref() == Some(&id))
                    && let Some(slot) = taken.get_mut(at)
                {
                    *slot = true;
                }
                if let Some(slot) = ids.get_mut(index) {
                    *slot = Some(id);
                }
            }
        }
        if in_place {
            for (index, name) in names.iter().enumerate() {
                let renamed = ids.get(index).is_some_and(Option::is_none)
                    && !taken.get(index).copied().unwrap_or(true)
                    && !name.contains("###")
                    && lines.get(index) == self.lines.get(index)
                    && self
                        .names
                        .get(index)
                        .is_some_and(|old| old != name && !names.contains(old));
                if !renamed {
                    continue;
                }
                let id = self.ids.get(index).cloned().flatten();
                if id
                    .as_ref()
                    .is_some_and(|id| ids.iter().flatten().any(|used| used == id))
                {
                    continue;
                }
                if let Some(slot) = taken.get_mut(index) {
                    *slot = true;
                }
                if let Some(slot) = ids.get_mut(index) {
                    *slot = id;
                }
            }
        }
        self.names = names;
        self.lines = lines;
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

/// The request line of `section`, its method and URL: what a rename in place leaves alone.
fn request_line(section: &DocumentRequest) -> String {
    format!("{} {}", section.method.as_str(), section.url)
}

/// The section of `text` the cursor `offset` falls in, as its index in `parse`'s order:
/// the last one that starts before the cursor when it sits between sections; `None`
/// before the first one, and in a section `parse` skips (a header still without its method
/// line), which has no index.
pub(super) fn section_at(text: &str, offset: usize) -> Option<usize> {
    let ranges = section_ranges(text);
    let at = ranges.iter().rposition(|range| range.start <= offset)?;
    let parsed = |range: &Range<usize>| {
        text.get(range.clone())
            .is_some_and(|section| !http_document::parse(section).0.is_empty())
    };
    if !ranges.get(at).is_some_and(parsed) {
        return None;
    }
    Some(ranges.iter().take(at).filter(|range| parsed(range)).count())
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
    use reqflux_core::http_document::DocumentRequest;
    use reqflux_core::model::Method;

    use super::{SectionIds, section_at};

    /// A section `name` whose request line is `GET /<url>`.
    fn section(name: &str, url: &str) -> DocumentRequest {
        DocumentRequest {
            name: name.to_owned(),
            method: Method::GET,
            url: format!("/{url}"),
            headers: Vec::new(),
            body: String::new(),
        }
    }

    /// Sections named `list`, each keeping the request line of the seed section at its
    /// place, so only the names change.
    fn names(list: &[&str]) -> Vec<DocumentRequest> {
        list.iter()
            .enumerate()
            .map(|(index, name)| section(name, &index.to_string()))
            .collect()
    }

    fn ids() -> SectionIds {
        SectionIds::new([("a", &section("Create", "0")), ("b", &section("List", "1"))])
    }

    #[test]
    fn a_rename_in_place_keeps_the_request() {
        let mut tracked = ids();
        tracked.track(&names(&["Create draft", "List"]));
        assert_eq!(tracked.id(0), Some("a"));
        assert_eq!(tracked.id(1), Some("b"));
    }

    #[test]
    fn moved_sections_match_by_name() {
        let mut tracked = ids();
        tracked.track(&names(&["List", "Create"]));
        assert_eq!(tracked.id(0), Some("b"));
        assert_eq!(tracked.id(1), Some("a"));
    }

    #[test]
    fn a_new_section_belongs_to_no_request() {
        let mut tracked = ids();
        tracked.track(&names(&["Health", "Create", "List"]));
        assert_eq!(tracked.id(0), None);
        assert_eq!(tracked.index_of("a"), Some(1));
        assert_eq!(tracked.index_of("b"), Some(2));
    }

    #[test]
    fn a_deleted_section_leaves_its_request_without_one() {
        let mut tracked = ids();
        tracked.track(&names(&["List"]));
        assert_eq!(tracked.index_of("a"), None);
        assert_eq!(tracked.index_of("b"), Some(0));
    }

    #[test]
    fn typing_a_section_above_a_header_keeps_the_ties() {
        let mut tracked = ids();
        tracked.track(&names(&["Create", "New### List"]));
        assert_eq!(tracked.id(1), None, "a glued header is ambiguous");
        tracked.track(&names(&["Create", "New", "List"]));
        assert_eq!(tracked.id(0), Some("a"));
        assert_eq!(tracked.id(1), None);
        assert_eq!(tracked.id(2), Some("b"));
    }

    #[test]
    fn typing_a_section_at_the_top_keeps_the_ties() {
        let mut tracked = ids();
        tracked.track(&names(&["New### Create", "List"]));
        assert_eq!(tracked.id(0), None);
        tracked.track(&names(&["New", "Create", "List"]));
        assert_eq!(tracked.id(0), None);
        assert_eq!(tracked.id(1), Some("a"));
        assert_eq!(tracked.id(2), Some("b"));
    }

    #[test]
    fn typing_a_section_after_the_last_keeps_the_ties() {
        let mut tracked = ids();
        tracked.track(&names(&["Create", "List", "N"]));
        tracked.track(&names(&["Create", "List", "New"]));
        assert_eq!(tracked.id(0), Some("a"));
        assert_eq!(tracked.id(1), Some("b"));
        assert_eq!(tracked.id(2), None);
    }

    #[test]
    fn two_renames_at_once_keep_their_positions() {
        let mut tracked = ids();
        tracked.track(&names(&["Create draft", "List all"]));
        assert_eq!(tracked.id(0), Some("a"));
        assert_eq!(tracked.id(1), Some("b"));
    }

    #[test]
    fn a_section_merged_from_two_belongs_to_neither() {
        let mut tracked = ids();
        tracked.track(&names(&["Crst"]));
        assert_eq!(tracked.id(0), None);
        assert_eq!(tracked.index_of("a"), None);
        assert_eq!(tracked.index_of("b"), None);
    }

    #[test]
    fn the_cursor_in_a_section_parse_skips_is_in_no_section() {
        let text = "### Create\nPOST /a\n\n### New\n### List\nGET /b\n";
        assert_eq!(section_at(text, 2), Some(0));
        assert_eq!(
            section_at(text, text.find("### New").unwrap_or(0) + 2),
            None
        );
        assert_eq!(
            section_at(text, text.find("### List").unwrap_or(0) + 2),
            Some(1)
        );
    }

    #[test]
    fn a_header_overwritten_by_a_typed_line_is_not_a_rename() {
        let mut tracked = ids();
        tracked.track(&[section("Create", "0"), section("New", "### List")]);
        assert_eq!(tracked.id(1), None);
        assert_eq!(tracked.index_of("b"), None);
    }
}
