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
/// A space counts as 1. A tab counts as `tab_width`.
/// A width of 0 counts as 1.
/// A blank line stays inside the current fold.
/// `end` is the first later non-blank line whose indent is less than or equal to the start line.
/// When no such line exists, `end` is the line count.
#[cfg_attr(not(test), allow(dead_code))]
pub fn indent_folds(text: &str, tab_width: u8) -> Vec<FoldSpan> {
    let _ = (text, tab_width);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{FoldSpan, indent_folds};

    #[test]
    fn nested_indent_makes_two_spans() {
        let text = "a\n  b\n    c\n  d\ne";
        assert_eq!(
            indent_folds(text, 4),
            vec![FoldSpan { start: 0, end: 4 }, FoldSpan { start: 1, end: 3 },]
        );
    }
}
