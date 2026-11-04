// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Line folds from leading indent.
//!
//! A line starts a fold when the next non-blank line is deeper.
//! A blank line does not start or end a fold.

/// One half-open line range.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldSpan {
    /// First line of the fold.
    pub start: u32,
    /// First line after the fold.
    pub end: u32,
}

/// Return half-open fold ranges for `text`.
///
/// A space counts as 1.
/// A tab counts as `tab_width`.
/// A width of 0 counts as 1.
/// A blank line stays inside the current fold.
/// An empty text has one line.
/// A trailing newline adds one blank line.
/// `end` is the first later non-blank line at an equal or smaller indent.
/// When no such line exists, `end` is the line count.
/// Spans are sorted by start line.
/// A longer span comes first when two spans share a start line.
#[cfg_attr(not(test), allow(dead_code))]
pub fn indent_folds(text: &str, tab_width: u8) -> Vec<FoldSpan> {
    let lines = line_indents(text, tab_width);
    let mut spans = Vec::new();
    for start in 0..lines.len() {
        if let Some(span) = fold_at(&lines, start) {
            spans.push(span);
        }
    }
    spans.sort_by(by_start_then_longer);
    spans
}

fn by_start_then_longer(left: &FoldSpan, right: &FoldSpan) -> std::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then(span_len(right).cmp(&span_len(left)))
}

fn span_len(span: &FoldSpan) -> u32 {
    span.end.saturating_sub(span.start)
}

struct LineIndent {
    blank: bool,
    indent: u32,
}

fn line_indents(text: &str, tab_width: u8) -> Vec<LineIndent> {
    let mut lines: Vec<LineIndent> = text
        .lines()
        .map(|line| line_indent(line, tab_width))
        .collect();
    if text.is_empty() || text.ends_with('\n') {
        lines.push(LineIndent {
            blank: true,
            indent: 0,
        });
    }
    lines
}

fn line_indent(line: &str, tab_width: u8) -> LineIndent {
    match leading_indent(line.trim_end_matches('\r'), tab_width) {
        Some(indent) => LineIndent {
            blank: false,
            indent,
        },
        None => LineIndent {
            blank: true,
            indent: 0,
        },
    }
}

/// Return the leading indent, or `None` when the line is blank.
fn leading_indent(line: &str, tab_width: u8) -> Option<u32> {
    let step = u32::from(tab_width.max(1));
    let mut indent = 0u32;
    for ch in line.chars() {
        match ch {
            ' ' => indent = indent.saturating_add(1),
            '\t' => indent = indent.saturating_add(step),
            _ => return Some(indent),
        }
    }
    None
}

fn fold_at(lines: &[LineIndent], start: usize) -> Option<FoldSpan> {
    if lines[start].blank {
        return None;
    }
    let next = next_content(lines, start + 1)?;
    if lines[next].indent <= lines[start].indent {
        return None;
    }
    Some(FoldSpan {
        start: line_no(start),
        end: line_no(fold_end(lines, start)),
    })
}

fn next_content(lines: &[LineIndent], from: usize) -> Option<usize> {
    (from..lines.len()).find(|&index| !lines[index].blank)
}

fn fold_end(lines: &[LineIndent], start: usize) -> usize {
    let indent = lines[start].indent;
    (start + 1..lines.len())
        .find(|&index| !lines[index].blank && lines[index].indent <= indent)
        .unwrap_or(lines.len())
}

fn line_no(index: usize) -> u32 {
    u32::try_from(index).expect("a line index fits in u32")
}

/// Return the innermost fold that strictly contains `line`.
///
/// A span contains `line` when `line > start && line < end`.
/// The innermost is the one with the greatest `start`.
/// When two spans share that start, the shorter one wins.
#[cfg_attr(not(test), allow(dead_code))]
pub fn innermost_fold(spans: &[FoldSpan], line: u32) -> Option<FoldSpan> {
    spans
        .iter()
        .copied()
        .filter(|span| line > span.start && line < span.end)
        .max_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| span_len(a).cmp(&span_len(b)).reverse())
        })
}

#[cfg(test)]
mod tests {
    use super::{FoldSpan, indent_folds, innermost_fold};

    #[test]
    fn nested_indent_makes_two_spans() {
        let text = "a\n  b\n    c\n  d\ne";
        assert_eq!(
            indent_folds(text, 4),
            vec![FoldSpan { start: 0, end: 4 }, FoldSpan { start: 1, end: 3 }]
        );
    }

    #[test]
    fn a_blank_line_stays_inside_the_outer_span() {
        let text = "a\n  b\n\n    c\n  d\ne";
        assert_eq!(
            indent_folds(text, 4),
            vec![FoldSpan { start: 0, end: 5 }, FoldSpan { start: 1, end: 4 },]
        );
    }

    #[test]
    fn a_whitespace_line_does_not_end_a_fold() {
        assert_eq!(
            indent_folds("a\n  b\n   \ne", 4),
            vec![FoldSpan { start: 0, end: 3 }]
        );
    }

    #[test]
    fn a_blank_line_does_not_start_a_fold() {
        assert_eq!(
            indent_folds("\na\n  b", 4),
            vec![FoldSpan { start: 1, end: 3 }]
        );
    }

    #[test]
    fn a_tab_counts_as_tab_width() {
        assert_eq!(
            indent_folds("a\n\tb\n\t\tc", 4),
            vec![FoldSpan { start: 0, end: 3 }, FoldSpan { start: 1, end: 3 },]
        );
    }

    #[test]
    fn a_zero_tab_width_counts_as_one() {
        assert_eq!(
            indent_folds("a\n\tb\n\t\tc", 0),
            vec![FoldSpan { start: 0, end: 3 }, FoldSpan { start: 1, end: 3 },]
        );
    }

    #[test]
    fn equal_indents_make_no_fold() {
        assert_eq!(indent_folds("a\nb\n", 4), Vec::new());
    }

    #[test]
    fn the_end_is_the_line_count_when_nothing_dedents() {
        assert_eq!(
            indent_folds("a\n  b", 4),
            vec![FoldSpan { start: 0, end: 2 }]
        );
    }

    #[test]
    fn a_trailing_newline_counts_as_a_line() {
        assert_eq!(
            indent_folds("a\n  b\n", 4),
            vec![FoldSpan { start: 0, end: 3 }]
        );
    }

    #[test]
    fn innermost_fold_picks_the_inner_span() {
        let spans = indent_folds("a\n  b\n    c\n  d\ne", 4);
        assert_eq!(
            spans,
            vec![FoldSpan { start: 0, end: 4 }, FoldSpan { start: 1, end: 3 }]
        );
        assert_eq!(innermost_fold(&spans, 0), None);
        assert_eq!(
            innermost_fold(&spans, 1),
            Some(FoldSpan { start: 0, end: 4 })
        );
        assert_eq!(
            innermost_fold(&spans, 2),
            Some(FoldSpan { start: 1, end: 3 })
        );
        assert_eq!(
            innermost_fold(&spans, 3),
            Some(FoldSpan { start: 0, end: 4 })
        );
        assert_eq!(innermost_fold(&spans, 4), None);
    }
}
