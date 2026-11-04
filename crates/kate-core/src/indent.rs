// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Newline indent and line indent.
//!
//! A newline copies the indent of the caret line.
//! When cstyle is on, a line that ends in `{`, `(`, or `[` adds one unit.
//! A Python newline reads the text before the caret.
//! It adds one unit when that text ends in `:`.
//! A comment line does not gain that unit.
//! `newline_for_indenter` picks one of those two results from the indenter name.
//! The name match ignores ASCII case.

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

/// Return the text a Python newline inserts at `at`.
///
/// The result starts with a newline.
/// It copies the leading spaces and tabs before the caret.
/// It ignores the text after the caret.
/// Spaces and tabs at the end of that prefix do not hide a colon.
/// When the last scalar of the prefix is a colon, the result adds one unit.
/// A comment line does not gain that unit.
/// A comment line is a line whose first non-indent scalar is `#`.
/// When the first word is return, pass, break, continue, or raise, the indent
/// drops one unit. A comment line does not drop.
/// A width of 0 counts as 1.
#[cfg_attr(not(test), allow(dead_code))]
pub fn python_newline(buf: &TextBuffer, at: Offset, config: &DocumentConfig) -> String {
    let prefix = text_before_caret(buf, at);
    let mut text = String::from("\n");
    let indent = leading_indent(&prefix);
    if !is_comment_line(&prefix) && is_dedent_word(&prefix, indent) && !ends_with_colon(&prefix) {
        text.push_str(without_one_unit(indent, config));
    } else if ends_with_colon(&prefix) && !is_comment_line(&prefix) {
        text.push_str(indent);
        text.push_str(&indent_unit(config));
    } else {
        text.push_str(indent);
    }
    text
}

/// Return the newline text for one indenter name.
///
/// The comparison uses ASCII case folding.
/// `python` calls [`python_newline`].
/// `cstyle` calls [`newline_insertion`] with the cstyle flag true.
/// `None` and any other name call [`newline_insertion`] with that flag false.
#[cfg_attr(not(test), allow(dead_code))]
pub fn newline_for_indenter(
    indenter: Option<&str>,
    buf: &TextBuffer,
    at: Offset,
    config: &DocumentConfig,
) -> String {
    match indenter {
        Some(name) if name.eq_ignore_ascii_case("python") => python_newline(buf, at, config),
        Some(name) if name.eq_ignore_ascii_case("cstyle") => {
            newline_insertion(buf, at, config, true)
        }
        _ => newline_insertion(buf, at, config, false),
    }
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

/// Build one delete of one indent step for each touched line.
///
/// A line that starts with the configured unit loses that unit.
/// A leading tab in spaces mode loses that one tab.
/// A leading space run loses up to one indent width.
/// A width of 0 counts as 1.
/// A shorter run is deleted in full.
/// One step does not delete a tab and spaces together.
/// A line with no indent produces no edit.
/// The result can be empty.
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

fn ends_with_colon(body: &str) -> bool {
    matches!(
        body.trim_end_matches([' ', '\t']).chars().next_back(),
        Some(':')
    )
}

/// A comment line is a line whose first non-indent scalar is `#`.
fn is_comment_line(body: &str) -> bool {
    matches!(
        body.trim_start_matches(is_indent_char).chars().next(),
        Some('#')
    )
}

fn is_dedent_word(prefix: &str, indent: &str) -> bool {
    let after = &prefix[indent.len()..];
    let end = after
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(after.len());
    matches!(
        &after[..end],
        "return" | "pass" | "break" | "continue" | "raise" | "else" | "elif" | "except" | "finally"
    )
}

fn without_one_unit<'a>(indent: &'a str, config: &DocumentConfig) -> &'a str {
    let unit = remove_unit(indent, config);
    if unit.is_empty() {
        indent
    } else {
        &indent[unit.len()..]
    }
}

/// Return the line text from the line start up to the caret.
///
/// The result ignores the text after the caret.
/// A trailing carriage return is not part of the result.
fn text_before_caret(buf: &TextBuffer, at: Offset) -> String {
    let line = buf.line_col(at).line;
    let start = buf.offset(LineCol { line, col: 0 });
    let line_end = buf.offset(LineCol {
        line,
        col: u32::MAX,
    });
    buf.slice(CharRange::new(start, at.min(line_end)))
        .to_string()
        .trim_end_matches('\r')
        .to_string()
}

fn remove_unit(body: &str, config: &DocumentConfig) -> String {
    let unit = indent_unit(config);
    if body.starts_with(&unit) {
        return unit;
    }
    if config.indent_with_spaces && body.starts_with('\t') {
        return "\t".to_string();
    }
    short_spaces(body, config)
}

/// Return a leading space run, up to one indent width.
///
/// A width of 0 counts as 1.
/// A shorter run is returned in full.
fn short_spaces(body: &str, config: &DocumentConfig) -> String {
    let width = usize::from(config.indent_width.max(1));
    let count = body.chars().take(width).take_while(|ch| *ch == ' ').count();
    " ".repeat(count)
}

#[cfg(test)]
mod tests {
    use super::{
        indent_lines, newline_for_indenter, newline_insertion, python_newline, unindent_lines,
    };
    use crate::DocumentConfig;
    use kate_text::{Edit, Offset, TextBuffer, Transaction};

    fn inserted(text: &str, config: &DocumentConfig, cstyle: bool) -> String {
        let buf = TextBuffer::from_str(text);
        newline_insertion(&buf, buf.len(), config, cstyle)
    }

    fn python_inserted(text: &str, config: &DocumentConfig) -> String {
        let buf = TextBuffer::from_str(text);
        python_newline(&buf, buf.len(), config)
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
    fn python_newline_adds_a_unit_after_a_colon() {
        let config = DocumentConfig::default();
        assert_eq!(python_inserted("def f():", &config), "\n    ");
        assert_eq!(python_inserted("def f()", &config), "\n");
        assert_eq!(python_inserted("    def f():", &config), "\n        ");
        assert_eq!(python_inserted("    def f()", &config), "\n    ");
        assert_eq!(python_inserted("\tdef f():", &config), "\n\t    ");
        assert_eq!(python_inserted("def f():  ", &config), "\n    ");
        assert_eq!(python_inserted("def f():\t", &config), "\n    ");
        assert_eq!(python_inserted("def f(): pass", &config), "\n");
        assert_eq!(python_inserted("def f():\r", &config), "\n    ");

        let tabs = DocumentConfig {
            indent_with_spaces: false,
            ..DocumentConfig::default()
        };
        assert_eq!(python_inserted("def f():", &tabs), "\n\t");
        assert_eq!(python_inserted("    def f():", &tabs), "\n    \t");

        let narrow = DocumentConfig {
            indent_width: 0,
            ..DocumentConfig::default()
        };
        assert_eq!(python_inserted("def f():", &narrow), "\n ");
        assert_eq!(python_inserted("def f()", &narrow), "\n");
    }

    #[test]
    fn python_newline_reads_only_the_text_before_the_caret() {
        let config = DocumentConfig::default();
        let buf = TextBuffer::from_str("def f(): pass");
        assert_eq!(python_newline(&buf, Offset(8), &config), "\n    ");
        let note = TextBuffer::from_str("# note:");
        assert_eq!(python_newline(&note, note.len(), &config), "\n");
        let indented = TextBuffer::from_str("    # note:");
        assert_eq!(python_newline(&indented, indented.len(), &config), "\n    ");
    }

    #[test]
    fn newline_for_indenter_picks_python() {
        let config = DocumentConfig::default();
        let python = TextBuffer::from_str("def f():");
        assert_eq!(
            newline_for_indenter(Some("python"), &python, python.len(), &config),
            "\n    "
        );
        let braced = TextBuffer::from_str("    foo{");
        assert_eq!(
            newline_for_indenter(Some("cstyle"), &braced, braced.len(), &config),
            "\n        "
        );
        let plain = TextBuffer::from_str("foo");
        assert_eq!(
            newline_for_indenter(None, &plain, plain.len(), &config),
            "\n"
        );
        assert_eq!(
            newline_for_indenter(Some("normal"), &braced, braced.len(), &config),
            "\n    "
        );
    }

    #[test]
    fn indenter_names_match_in_either_case() {
        let config = DocumentConfig::default();
        let python = TextBuffer::from_str("def f():");
        assert_eq!(
            newline_for_indenter(Some("Python"), &python, python.len(), &config),
            "\n    "
        );
        assert_eq!(
            newline_for_indenter(Some("PYTHON"), &python, python.len(), &config),
            "\n    "
        );
        let braced = TextBuffer::from_str("    foo{");
        assert_eq!(
            newline_for_indenter(Some("CStyle"), &braced, braced.len(), &config),
            "\n        "
        );
        assert_eq!(
            newline_for_indenter(Some("CSTYLE"), &braced, braced.len(), &config),
            "\n        "
        );
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

    #[test]
    fn a_bare_else_drops_one_indent() {
        let config = DocumentConfig::default();
        assert_eq!(python_inserted("    else", &config), "\n");
        assert_eq!(python_inserted("    else:", &config), "\n        ");
        assert_eq!(python_inserted("    elif x:", &config), "\n        ");
        assert_eq!(python_inserted("    except:", &config), "\n        ");
        assert_eq!(python_inserted("    finally:", &config), "\n        ");
        assert_eq!(python_inserted("    # else", &config), "\n    ");
        assert_eq!(python_inserted("def f():", &config), "\n    ");
    }

    #[test]
    fn a_return_line_drops_one_indent_unit() {
        let config = DocumentConfig::default();
        assert_eq!(python_inserted("    return", &config), "\n");
        assert_eq!(python_inserted("        return", &config), "\n    ");
        assert_eq!(python_inserted("    return x", &config), "\n");
        assert_eq!(python_inserted("    returning", &config), "\n    ");
        assert_eq!(python_inserted("    # return", &config), "\n    ");
        assert_eq!(python_inserted("def f():", &config), "\n    ");
        assert_eq!(python_inserted("def f(): pass", &config), "\n");

        let tabs = DocumentConfig {
            indent_with_spaces: false,
            ..DocumentConfig::default()
        };
        assert_eq!(python_inserted("\t\tpass", &tabs), "\n\t");

        assert_eq!(python_inserted("    break", &config), "\n");
        assert_eq!(python_inserted("    continue", &config), "\n");
        assert_eq!(python_inserted("    raise", &config), "\n");
    }
}
