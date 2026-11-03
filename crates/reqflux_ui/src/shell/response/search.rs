//! Search in the response body without GPUI: the matches of a query in the body's lines,
//! the `n / total` count and stepping between matches; the body view draws them over the
//! syntax colors (see `syntax`).

/// One match: the line it is on, from 0, and its byte range in that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hit {
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// Every match of `query` in `lines`, top to bottom and left to right, case-insensitive and
/// not overlapping. An empty query matches nothing.
pub(crate) fn find(lines: &[String], query: &str) -> Vec<Hit> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let mut from = 0;
        while let Some(rest) = line.get(from..) {
            let found = rest.char_indices().find_map(|(offset, _)| {
                let tail = rest.get(offset..)?;
                match_len(tail, query).map(|len| (offset, len))
            });
            let Some((offset, len)) = found else {
                break;
            };
            let start = from + offset;
            hits.push(Hit {
                line: line_index,
                start,
                end: start + len,
            });
            from = start + len;
        }
    }
    hits
}

/// The length in bytes of the text `text` starts with that equals `query` ignoring case,
/// or `None` when it does not start with it.
fn match_len(text: &str, query: &str) -> Option<usize> {
    let mut text_chars = text.char_indices();
    let mut end = 0;
    for wanted in query.chars() {
        let (index, found) = text_chars.next()?;
        if !found.to_lowercase().eq(wanted.to_lowercase()) {
            return None;
        }
        end = index + found.len_utf8();
    }
    Some(end)
}

/// The count the search field shows: `<current> / <total>` from 1, `0 / 0` without
/// matches.
pub(crate) fn count_label(current: usize, total: usize) -> String {
    match total {
        0 => "0 / 0".to_owned(),
        total => format!("{} / {total}", current.min(total - 1) + 1),
    }
}

/// The match after (`forward`) or before `current` of `total`, wrapping around.
pub(crate) fn step(current: usize, total: usize, forward: bool) -> usize {
    match (total, forward) {
        (0, _) => 0,
        (total, true) => (current + 1) % total,
        (total, false) => (current % total + total - 1) % total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn matches_are_case_insensitive_in_line_order_and_never_overlap() {
        let body = lines(&["  \"id\": \"In_1\",", "  \"lines\": [", "aaaa"]);
        assert_eq!(
            find(&body, "in"),
            vec![
                Hit {
                    line: 0,
                    start: 9,
                    end: 11
                },
                Hit {
                    line: 1,
                    start: 4,
                    end: 6
                },
            ]
        );
        assert_eq!(find(&body, "aa").len(), 2, "aaaa holds two, not three");
        assert_eq!(find(&body, ""), Vec::new());
        assert_eq!(find(&body, "absent"), Vec::new());
    }

    #[test]
    fn a_match_keeps_byte_offsets_after_wide_characters() {
        let body = lines(&["héllo HÉLLO"]);
        assert_eq!(
            find(&body, "éll"),
            vec![
                Hit {
                    line: 0,
                    start: 1,
                    end: 5
                },
                Hit {
                    line: 0,
                    start: 8,
                    end: 12
                },
            ]
        );
    }

    #[test]
    fn the_count_reads_current_of_total() {
        assert_eq!(count_label(0, 2), "1 / 2");
        assert_eq!(count_label(1, 2), "2 / 2");
        assert_eq!(count_label(0, 0), "0 / 0");
    }

    #[test]
    fn next_and_previous_wrap_around() {
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(1, 3, false), 0);
        assert_eq!(step(0, 0, true), 0);
    }
}
