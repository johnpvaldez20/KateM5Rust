// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! One view of a document.
//!
//! The view owns selections and scroll state.
//! It does not own the text.

use kate_text::{CharRange, Offset};

use crate::document::DocumentId;
use crate::selection::Selections;

#[path = "fold.rs"]
mod fold;

slotmap::new_key_type! {
    /// Key for one view in a [`crate::Workspace`].
    pub struct ViewId;
}

/// One view of a document.
#[derive(Clone, Debug)]
pub struct View {
    /// Document this view shows.
    pub document: DocumentId,
    /// Carets and ranges for this view.
    pub selections: Selections,
    /// First visible line and column.
    pub scroll: Scroll,
    /// Folded character ranges. A new view starts empty.
    pub folds: Vec<CharRange>,
    /// `true` when the view wraps long lines.
    pub dynamic_wrap: bool,
}

/// Scroll position in logical lines and columns.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Scroll {
    /// First visible line.
    pub first_line: u32,
    /// First visible column.
    pub first_col: u32,
}

impl View {
    /// Store one character range for each indent fold.
    ///
    /// A span starts at the line break that ends its header line.
    /// The header text stays outside the range.
    /// A span ends at the first scalar of its end line.
    /// When the end line is the line count, the end is the scalar count.
    pub fn apply_indent_folds(&mut self, text: &str, tab_width: u8) {
        let normalized: String = text.replace("\r\n", "\n").replace('\r', "\n");
        self.folds = fold::indent_folds(&normalized, tab_width)
            .into_iter()
            .map(|span| char_range_for_span(text, span))
            .collect();
    }

    /// Return true when `at` is inside a stored fold range.
    ///
    /// The check uses the ranges from `apply_indent_folds`.
    /// A caret on the header text is false.
    /// The line break that starts the range is true.
    /// The end offset is false.
    pub fn offset_is_folded(&self, at: Offset) -> bool {
        self.folds.iter().any(|range| range.contains(at))
    }

    /// Return the shortest stored range that contains `at`.
    ///
    /// When two ranges have the same length, the one with the later start wins.
    /// Returns `None` when no stored range contains `at`.
    pub fn folded_range_at(&self, at: Offset) -> Option<CharRange> {
        self.folds
            .iter()
            .filter(|range| range.contains(at))
            .min_by(|a, b| {
                a.len()
                    .cmp(&b.len())
                    .then_with(|| b.start().cmp(&a.start()))
            })
            .copied()
    }

    /// Return how many stored ranges contain `at`.
    ///
    /// The end offset of a range does not count.
    pub fn fold_covering_count(&self, at: Offset) -> usize {
        self.folds.iter().filter(|range| range.contains(at)).count()
    }

    /// Return `true` when the first line or the first column is not zero.
    ///
    /// A new view is not scrolled.
    pub fn is_scrolled(&self) -> bool {
        self.scroll.first_line != 0 || self.scroll.first_col != 0
    }
}

fn char_range_for_span(text: &str, span: fold::FoldSpan) -> CharRange {
    CharRange::new(
        Offset(header_line_break(text, span.start)),
        Offset(line_scalar(text, span.end)),
    )
}

/// Return the line break that ends the header `line`.
///
/// The header text stays before this scalar.
fn header_line_break(text: &str, line: u32) -> usize {
    line_scalar(text, line.saturating_add(1)).saturating_sub(1)
}

/// Return the first scalar of `line`.
///
/// Line 0 starts at scalar 0.
/// Each line break starts the next line, and a CR LF pair is one break.
/// When `line` is the line count, the result is the scalar count.
fn line_scalar(text: &str, line: u32) -> usize {
    let mut newlines = 0u32;
    let mut count = 0usize;
    let mut skip_lf = false;
    for ch in text.chars() {
        if newlines == line {
            return count;
        }
        count += 1;
        if skip_lf {
            skip_lf = false;
        } else if ch == '\r' || ch == '\n' {
            newlines += 1;
            skip_lf = ch == '\r';
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use kate_text::{CharRange, Offset};

    use crate::document::DocumentId;
    use crate::selection::{Selection, Selections};

    use super::{Scroll, View};

    #[test]
    fn apply_indent_folds_stores_char_ranges() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        view.apply_indent_folds("a\n  b\n", 4);
        assert_eq!(view.folds, vec![CharRange::new(Offset(1), Offset(6))]);
        assert!(!view.folds[0].contains(Offset(0)));
    }

    #[test]
    fn the_header_text_is_outside_the_fold() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        view.apply_indent_folds("a\n  b\n", 4);
        assert!(!view.offset_is_folded(Offset(0)));
        assert!(view.offset_is_folded(Offset(1)));
        assert!(view.offset_is_folded(Offset(5)));
        assert!(!view.offset_is_folded(Offset(6)));
    }

    #[test]
    fn folded_range_at_picks_the_shortest() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        view.apply_indent_folds("a\n  b\n", 4);
        assert_eq!(view.folded_range_at(Offset(0)), None);
        assert_eq!(
            view.folded_range_at(Offset(1)),
            Some(CharRange::new(Offset(1), Offset(6)))
        );
        assert_eq!(
            view.folded_range_at(Offset(5)),
            Some(CharRange::new(Offset(1), Offset(6)))
        );
        assert_eq!(view.folded_range_at(Offset(6)), None);

        view.folds = vec![
            CharRange::new(Offset(0), Offset(10)),
            CharRange::new(Offset(2), Offset(5)),
        ];
        assert_eq!(
            view.folded_range_at(Offset(3)),
            Some(CharRange::new(Offset(2), Offset(5)))
        );
        assert_eq!(
            view.folded_range_at(Offset(1)),
            Some(CharRange::new(Offset(0), Offset(10)))
        );
        assert_eq!(view.folded_range_at(Offset(10)), None);

        view.folds = vec![
            CharRange::new(Offset(0), Offset(4)),
            CharRange::new(Offset(1), Offset(5)),
        ];
        assert_eq!(
            view.folded_range_at(Offset(2)),
            Some(CharRange::new(Offset(1), Offset(5)))
        );
    }

    #[test]
    fn fold_covering_count_counts_every_range() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        view.apply_indent_folds("a\n  b\n", 4);
        assert_eq!(view.fold_covering_count(Offset(0)), 0);
        assert_eq!(view.fold_covering_count(Offset(1)), 1);
        assert_eq!(view.fold_covering_count(Offset(5)), 1);
        assert_eq!(view.fold_covering_count(Offset(6)), 0);

        view.folds = vec![
            CharRange::new(Offset(0), Offset(10)),
            CharRange::new(Offset(2), Offset(5)),
        ];
        assert_eq!(view.fold_covering_count(Offset(3)), 2);
        assert_eq!(view.fold_covering_count(Offset(1)), 1);
        assert_eq!(view.fold_covering_count(Offset(10)), 0);
    }

    #[test]
    fn a_new_view_is_not_scrolled() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        assert!(!view.is_scrolled());
        view.scroll.first_line = 2;
        assert!(view.is_scrolled());
        view.scroll.first_line = 0;
        view.scroll.first_col = 3;
        assert!(view.is_scrolled());
    }

    #[test]
    fn a_carriage_return_folds_like_a_newline() {
        let mut view = View {
            document: DocumentId::default(),
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        };
        view.apply_indent_folds("a\r  b\r", 4);
        assert_eq!(view.folds, vec![CharRange::new(Offset(1), Offset(6))]);

        view.folds.clear();
        view.apply_indent_folds("a\n  b\n", 4);
        assert_eq!(view.folds, vec![CharRange::new(Offset(1), Offset(6))]);
    }
}
