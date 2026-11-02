//! The docs as one static HTML page (`index.html`): inline styles, no script, no network.

use super::Block;

const STYLE: &str = "\
body{font:15px/1.5 system-ui,sans-serif;margin:0;display:flex;color:#1d2330}\
nav{width:280px;flex:none;height:100vh;overflow:auto;position:sticky;top:0;padding:16px;\
box-sizing:border-box;border-right:1px solid #dde1ea;background:#f6f7fa;font-size:13px}\
nav a{display:block;color:inherit;text-decoration:none;padding:2px 0}\
main{flex:1;padding:24px 40px;max-width:960px}\
code,pre{font:13px/1.45 ui-monospace,monospace;background:#f1f3f7;border-radius:4px}\
code{padding:2px 6px}pre{padding:12px;overflow:auto}\
table{border-collapse:collapse;margin:4px 0 12px}\
th,td{border:1px solid #dde1ea;padding:4px 8px;text-align:left;vertical-align:top}\
h1,h2,h3,h4,h5,h6{margin:28px 0 8px}";

/// Renders the blocks as a page titled `title`, with a navigation list of its headings.
pub(super) fn render(title: &str, blocks: &[Block]) -> String {
    let mut nav = String::new();
    let mut main = String::new();
    for block in blocks {
        match block {
            Block::Heading {
                level,
                anchor,
                text,
            } => {
                let indent = level.saturating_sub(1) * 12;
                nav.push_str(&format!(
                    "<a href=\"#{anchor}\" style=\"padding-left:{indent}px\">{}</a>\n",
                    escape(text)
                ));
                main.push_str(&format!(
                    "<h{level} id=\"{anchor}\">{}</h{level}>\n",
                    escape(text)
                ));
            }
            Block::Text(text) => {
                main.push_str(&format!("<p>{}</p>\n", escape(text)));
            }
            Block::Call(text) => {
                main.push_str(&format!("<p><code>{}</code></p>\n", escape(text)));
            }
            Block::Code(text) => {
                main.push_str(&format!("<pre>{}</pre>\n", escape(text)));
            }
            Block::Table { title, head, rows } => {
                main.push_str(&format!(
                    "<p><strong>{}</strong></p>\n<table>\n<tr>",
                    escape(title)
                ));
                for cell in head {
                    main.push_str(&format!("<th>{}</th>", escape(cell)));
                }
                main.push_str("</tr>\n");
                for cells in rows {
                    main.push_str("<tr>");
                    for cell in cells {
                        main.push_str(&format!("<td>{}</td>", escape(cell)));
                    }
                    main.push_str("</tr>\n");
                }
                main.push_str("</table>\n");
            }
        }
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<nav>\n{nav}</nav>\n\
<main>\n{main}</main>\n</body>\n</html>\n",
        escape(title)
    )
}

/// Escapes text for an HTML element or a double-quoted attribute.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_markup_and_quotes() {
        assert_eq!(
            escape("<a href=\"x\">'&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }
}
