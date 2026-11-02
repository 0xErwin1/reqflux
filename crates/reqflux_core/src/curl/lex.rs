//! Splits a pasted command into words, with bash or Windows `cmd` quoting.

use super::CurlError;

/// The words of `command`. A command written for `cmd` (a `^"` quote or a line ending in
/// `^`, as Chrome's "Copy as cURL (cmd)" writes) uses [`cmd_words`]; any other uses
/// [`bash_words`].
pub(super) fn words(command: &str) -> Result<Vec<String>, CurlError> {
    let cmd = command.contains("^\"") || command.lines().any(|line| line.trim_end().ends_with('^'));
    if cmd {
        Ok(cmd_words(command))
    } else {
        bash_words(command)
    }
}

/// Bash words: `'single'` quotes are literal, `$'ansi-c'` quotes decode escapes, `"double"`
/// quotes decode `\"`, `\\`, `\$` and `` \` ``, a backslash escapes the next character
/// outside quotes, a backslash before a newline continues the line and `#` at the start of a
/// word comments out the rest of the line. Nothing is expanded: `$HOME` stays as written.
fn bash_words(command: &str) -> Result<Vec<String>, CurlError> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if let Some(done) = word.take() {
                    words.push(done);
                }
            }
            '#' if word.is_none() => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\r') if chars.peek() == Some(&'\n') => {
                    chars.next();
                }
                Some('\n') => {}
                Some(escaped) => word.get_or_insert_with(String::new).push(escaped),
                None => return Err(CurlError::Unterminated),
            },
            '\'' => {
                let text = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => text.push(inner),
                        None => return Err(CurlError::Unterminated),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                ansi_c(&mut chars, word.get_or_insert_with(String::new))?;
            }
            '"' => {
                let text = word.get_or_insert_with(String::new);
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '$' | '`')) => text.push(escaped),
                            Some('\n') => {}
                            Some(other) => {
                                text.push('\\');
                                text.push(other);
                            }
                            None => return Err(CurlError::Unterminated),
                        },
                        Some(inner) => text.push(inner),
                        None => return Err(CurlError::Unterminated),
                    }
                }
            }
            other => word.get_or_insert_with(String::new).push(other),
        }
    }
    words.extend(word);
    Ok(words)
}

/// The rest of a `$'...'` quote, after the opening quote, decoded into `text`.
fn ansi_c(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    text: &mut String,
) -> Result<(), CurlError> {
    loop {
        let Some(c) = chars.next() else {
            return Err(CurlError::Unterminated);
        };
        match c {
            '\'' => return Ok(()),
            '\\' => {
                let Some(escaped) = chars.next() else {
                    return Err(CurlError::Unterminated);
                };
                match escaped {
                    'n' => text.push('\n'),
                    't' => text.push('\t'),
                    'r' => text.push('\r'),
                    'a' => text.push('\u{7}'),
                    'b' => text.push('\u{8}'),
                    'e' | 'E' => text.push('\u{1b}'),
                    'f' => text.push('\u{c}'),
                    'v' => text.push('\u{b}'),
                    'x' => push_code(chars, text, 16, 2, "\\x"),
                    'u' => push_code(chars, text, 16, 4, "\\u"),
                    'U' => push_code(chars, text, 16, 8, "\\U"),
                    '0'..='7' => {
                        let mut code = escaped.to_digit(8).unwrap_or(0);
                        for _ in 0..2 {
                            match chars.peek().and_then(|next| next.to_digit(8)) {
                                Some(digit) => {
                                    code = code * 8 + digit;
                                    chars.next();
                                }
                                None => break,
                            }
                        }
                        text.extend(char::from_u32(code));
                    }
                    other => text.push(other),
                }
            }
            other => text.push(other),
        }
    }
}

/// Up to `max` digits in `radix` as one character; with no digit, `prefix` as written.
fn push_code(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    text: &mut String,
    radix: u32,
    max: usize,
    prefix: &str,
) {
    let mut code = 0;
    let mut digits = 0;
    while digits < max {
        match chars.peek().and_then(|next| next.to_digit(radix)) {
            Some(digit) => {
                code = code * radix + digit;
                digits += 1;
                chars.next();
            }
            None => break,
        }
    }
    if digits == 0 {
        text.push_str(prefix);
    } else {
        text.extend(char::from_u32(code));
    }
}

/// `cmd` words in two passes, as Windows runs them: `cmd.exe` drops each `^` and keeps the
/// character after it (a `^` before a line break continues the line, and a second line break
/// right after it is a literal one), then the C runtime splits at whitespace outside `"`
/// quotes, where `\"` is a literal quote and `\\` a backslash.
fn cmd_words(command: &str) -> Vec<String> {
    let mut unescaped = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '^' {
            unescaped.push(c);
            continue;
        }
        match chars.next() {
            Some('\r') if chars.peek() == Some(&'\n') => {
                chars.next();
                continue_line(&mut chars, &mut unescaped);
            }
            Some('\n') => continue_line(&mut chars, &mut unescaped),
            Some(escaped) => unescaped.push(escaped),
            None => {}
        }
    }

    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quoted = false;
    let mut chars = unescaped.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                word.get_or_insert_with(String::new);
                quoted = !quoted;
            }
            '\\' if matches!(chars.peek(), Some('"' | '\\')) => {
                word.get_or_insert_with(String::new).extend(chars.next());
            }
            c if c.is_whitespace() && !quoted => {
                if let Some(done) = word.take() {
                    words.push(done);
                }
            }
            other => word.get_or_insert_with(String::new).push(other),
        }
    }
    words.extend(word);
    words
}

/// After a `^` line continuation: a line break right after it is a literal one.
fn continue_line(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    if chars.peek() == Some(&'\r') {
        chars.next();
    }
    if chars.peek() == Some(&'\n') {
        chars.next();
        out.push('\n');
    }
}
