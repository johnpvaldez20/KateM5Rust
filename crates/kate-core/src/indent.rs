// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Newline indent and line indent.
//!
//! A newline copies the indent of the caret line.
//! When cstyle is on, a line that ends in `{`, `(`, or `[` adds one unit.

use std::collections::BTreeSet;

use kate_text::{CharRange, Edit, LineCol, Offset, TextBuffer, Transaction, TransactionError};

use crate::DocumentConfig;

/// Return one indent step for `config`.
///
/// Spaces repeat `indent_width` times. A width of 0 counts as 1.
/// When `indent_with_spaces` is false, the step is one tab.
pub fn indent_unit(config: &DocumentConfig) -> String {
    if config.indent_with_spaces {
        let count = usize::from(config.indent_width.max(1));
        return " ".repeat(count);
    }
    "\t".to_string()
}

/// Return the text a newline inserts at `at`.
///
/// The result starts with a newline.
/// It then copies the leading spaces and tabs on that line.
/// When `cstyle` is true, an opener at the end of the line adds one unit.
/// Trailing spaces do not hide `{`, `(`, or `[`.
pub fn newline_insertion(
    buf: &TextBuffer,
    at: Offset,
    config: &DocumentConfig,
    cstyle: bool,
) -> String {
    let body = line_body(buf, buf.line_col(at).line);
    let mut text = String::from("\n");
    text.push_str(leading_indent(&body));
    if cstyle && ends_with_opener(&body) {
        text.push_str(&indent_unit(config));
    }
    text
}

/// Build one insert of [`indent_unit`] for each touched line.
///
/// A repeated line produces one edit. A line past the buffer produces no edit.
pub fn indent_lines(
    buf: &TextBuffer,
    lines: &[u32],
    config: &DocumentConfig,
) -> Result<Transaction, TransactionError> {
    let unit = indent_unit(config);
    let mut edits = Vec::new();
    for_each_line(buf, lines, |start, _body| {
        edits.push(Edit::insert(start, unit.clone()));
    });
    Transaction::new(edits)
}

/// Build one delete of one indent unit for each touched line.
///
/// A line with no indent produces no edit. The result can be empty.
pub fn unindent_lines(
    buf: &TextBuffer,
    lines: &[u32],
    config: &DocumentConfig,
) -> Result<Transaction, TransactionError> {
    let mut edits = Vec::new();
    for_each_line(buf, lines, |start, body| {
        let old = remove_unit(body, config);
        if !old.is_empty() {
            edits.push(Edit::delete(start, old));
        }
    });
    Transaction::new(edits)
}

fn for_each_line(buf: &TextBuffer, lines: &[u32], mut visit: impl FnMut(Offset, &str)) {
    let mut seen = BTreeSet::new();
    for line in lines {
        if *line >= buf.line_count() || !seen.insert(*line) {
            continue;
        }
        let start = buf.offset(LineCol {
            line: *line,
            col: 0,
        });
        let body = line_body(buf, *line);
        visit(start, &body);
    }
}

fn line_body(buf: &TextBuffer, line: u32) -> String {
    let start = buf.offset(LineCol { line, col: 0 });
    let end = buf.offset(LineCol {
        line,
        col: u32::MAX,
    });
    buf.slice(CharRange::new(start, end))
        .to_string()
        .trim_end_matches('\r')
        .to_string()
}

fn leading_indent(body: &str) -> &str {
    let end = body
        .char_indices()
        .find(|(_, ch)| !is_indent_char(*ch))
        .map(|(index, _)| index)
        .unwrap_or(body.len());
    &body[..end]
}

fn is_indent_char(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

fn ends_with_opener(body: &str) -> bool {
    matches!(
        body.trim_end_matches([' ', '\t']).chars().next_back(),
        Some('{' | '(' | '[')
    )
}

fn remove_unit(body: &str, config: &DocumentConfig) -> String {
    let unit = indent_unit(config);
    if body.starts_with(&unit) {
        return unit;
    }
    short_spaces(body, config)
}

fn short_spaces(body: &str, config: &DocumentConfig) -> String {
    if !config.indent_with_spaces {
        return String::new();
    }
    let width = usize::from(config.indent_width.max(1));
    let count = body.chars().take(width).take_while(|ch| *ch == ' ').count();
    " ".repeat(count)
}

#[cfg(test)]
mod tests {
    use super::{indent_lines, newline_insertion, unindent_lines};
    use crate::DocumentConfig;
    use kate_text::{Edit, Offset, TextBuffer, Transaction};

    fn inserted(text: &str, config: &DocumentConfig, cstyle: bool) -> String {
        let buf = TextBuffer::from_str(text);
        newline_insertion(&buf, buf.len(), config, cstyle)
    }

    #[test]
    fn plain_newline_copies_the_indent() {
        let config = DocumentConfig::default();
        let got = inserted("    foo{", &config, false);
        assert_eq!(got, "\n    ");
        let mut buf = TextBuffer::from_str("    foo{");
        let at = buf.len();
        buf.apply(&Transaction::single(Edit::insert(at, got)))
            .expect("insert");
        assert_eq!(buf.text().to_string(), "    foo{\n    ");
        assert_eq!(inserted("\t\tfoo{", &config, false), "\n\t\t");
        let buf = TextBuffer::from_str("    foo{");
        assert_eq!(newline_insertion(&buf, Offset(2), &config, false), "\n    ");
    }

    #[test]
    fn cstyle_newline_adds_one_unit_after_a_brace() {
        let config = DocumentConfig::default();
        assert_eq!(inserted("    foo{", &config, true), "\n        ");
        let buf = TextBuffer::from_str("    foo{");
        assert_eq!(
            newline_insertion(&buf, Offset(6), &config, true),
            "\n        "
        );
        assert_eq!(inserted("    foo", &config, true), "\n    ");
        assert_eq!(inserted("    foo(", &config, true), "\n        ");
        assert_eq!(inserted("    foo[", &config, true), "\n        ");
        assert_eq!(inserted("    foo{  ", &config, true), "\n        ");
        assert_eq!(inserted("    foo}", &config, true), "\n    ");
        assert_eq!(inserted("    foo{\r", &config, true), "\n        ");

        let tabs = DocumentConfig {
            indent_with_spaces: false,
            ..DocumentConfig::default()
        };
        assert_eq!(inserted("    foo{", &tabs, true), "\n    \t");

        let narrow = DocumentConfig {
            indent_width: 0,
            ..DocumentConfig::default()
        };
        assert_eq!(inserted("    foo{", &narrow, true), "\n     ");
    }

    #[test]
    fn indent_lines_edits_each_touched_line_once() {
        let config = DocumentConfig::default();
        let buf = TextBuffer::from_str("ab\n  cd");
        let tx = indent_lines(&buf, &[0, 1, 1], &config).expect("indent");
        assert_eq!(tx.edits().len(), 2);
        assert_eq!(tx.edits()[0].start, Offset(0));
        assert_eq!(tx.edits()[0].new, "    ");
        assert_eq!(tx.edits()[1].new, "    ");
        let none = unindent_lines(&buf, &[0], &config).expect("plain line");
        assert!(none.is_empty());
        let back = unindent_lines(&TextBuffer::from_str("    ab"), &[0], &config).expect("unit");
        assert_eq!(back.edits().len(), 1);
        assert_eq!(back.edits()[0].old, "    ");
        let partial = unindent_lines(&TextBuffer::from_str("  ab"), &[0], &config).expect("part");
        assert_eq!(partial.edits()[0].old, "  ");
    }

    #[test]
    fn spaces_mode_strips_one_tab() {
        let config = DocumentConfig::default();
        let tab = unindent_lines(&TextBuffer::from_str("\tab"), &[0], &config).expect("tab");
        assert_eq!(tab.edits().len(), 1);
        assert_eq!(tab.edits()[0].start, Offset(0));
        assert_eq!(tab.edits()[0].old, "\t");
        assert_eq!(tab.edits()[0].new, "");
        let mixed =
            unindent_lines(&TextBuffer::from_str("\t    ab"), &[0], &config).expect("mixed");
        assert_eq!(mixed.edits().len(), 1);
        assert_eq!(mixed.edits()[0].old, "\t");
        let spaces =
            unindent_lines(&TextBuffer::from_str("    ab"), &[0], &config).expect("spaces");
        assert_eq!(spaces.edits().len(), 1);
        assert_eq!(spaces.edits()[0].old, "    ");
    }

    #[test]
    fn tab_mode_strips_leading_spaces() {
        let config = DocumentConfig {
            indent_with_spaces: false,
            indent_width: 4,
            ..DocumentConfig::default()
        };
        let four = unindent_lines(&TextBuffer::from_str("    ab"), &[0], &config).expect("four");
        assert_eq!(four.edits().len(), 1);
        assert_eq!(four.edits()[0].start, Offset(0));
        assert_eq!(four.edits()[0].old, "    ");
        assert_eq!(four.edits()[0].new, "");
        let two = unindent_lines(&TextBuffer::from_str("  \tab"), &[0], &config).expect("two");
        assert_eq!(two.edits().len(), 1);
        assert_eq!(two.edits()[0].old, "  ");
        let tab = unindent_lines(&TextBuffer::from_str("\tab"), &[0], &config).expect("tab");
        assert_eq!(tab.edits().len(), 1);
        assert_eq!(tab.edits()[0].old, "\t");
        let narrow = DocumentConfig {
            indent_with_spaces: false,
            indent_width: 0,
            ..DocumentConfig::default()
        };
        let one = unindent_lines(&TextBuffer::from_str("  ab"), &[0], &narrow).expect("narrow");
        assert_eq!(one.edits().len(), 1);
        assert_eq!(one.edits()[0].old, " ");
    }
}
