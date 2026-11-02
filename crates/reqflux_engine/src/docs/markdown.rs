//! The docs as Markdown (`README.md`).

use super::Block;

/// Renders the blocks as CommonMark with GitHub tables.
pub(super) fn render(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            Block::Heading { level, text, .. } => {
                out.push_str(&"#".repeat(*level));
                out.push(' ');
                out.push_str(&one_line(text));
                out.push('\n');
            }
            Block::Text(text) => out.push_str(text),
            Block::Call(text) => out.push_str(&code_span(text)),
            Block::Code(text) => {
                let fence = fence(text);
                out.push_str(&fence);
                out.push('\n');
                out.push_str(text);
                if !text.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&fence);
            }
            Block::Table { title, head, rows } => {
                out.push_str(&format!("**{title}**\n\n"));
                out.push_str(&row(&head.map(str::to_owned)));
                out.push_str("|---|---|---|\n");
                for cells in rows {
                    out.push_str(&row(cells));
                }
                // The row writer ends with a newline; the block separator adds the blank line.
                out.pop();
            }
        }
        out.push_str("\n\n");
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

fn row(cells: &[String; 3]) -> String {
    let cells: Vec<String> = cells.iter().map(|cell| cell_text(cell)).collect();
    format!("| {} |\n", cells.join(" | "))
}

/// A table cell: one line, `|` escaped.
fn cell_text(text: &str) -> String {
    one_line(text).replace('|', "\\|")
}

fn one_line(text: &str) -> String {
    text.split(['\r', '\n']).collect::<Vec<_>>().join(" ")
}

/// Inline code that holds any backticks in `text`.
fn code_span(text: &str) -> String {
    let text = one_line(text);
    let longest = longest_run(&text, '`');
    if longest == 0 {
        return format!("`{text}`");
    }
    let ticks = "`".repeat(longest + 1);
    format!("{ticks} {text} {ticks}")
}

/// A fence longer than any backtick run in `text`.
fn fence(text: &str) -> String {
    "`".repeat(longest_run(text, '`').max(2) + 1)
}

fn longest_run(text: &str, ch: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        if c == ch {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_keeps_backticks_inside() {
        assert_eq!(code_span("GET /a"), "`GET /a`");
        assert_eq!(code_span("a`b"), "`` a`b ``");
        assert_eq!(fence("x"), "```");
        assert_eq!(fence("```js"), "````");
    }

    #[test]
    fn cells_stay_on_one_line_and_escape_pipes() {
        assert_eq!(cell_text("a|b\nc"), "a\\|b c");
    }
}
