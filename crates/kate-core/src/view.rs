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
    /// A span starts at the first scalar of its start line.
    /// A span ends at the first scalar of its end line.
    /// When the end line is the line count, the end is the scalar count.
    pub fn apply_indent_folds(&mut self, text: &str, tab_width: u8) {
        self.folds = fold::indent_folds(text, tab_width)
            .into_iter()
            .map(|span| char_range_for_span(text, span))
            .collect();
    }
}

fn char_range_for_span(text: &str, span: fold::FoldSpan) -> CharRange {
    CharRange::new(
        Offset(line_scalar(text, span.start)),
        Offset(line_scalar(text, span.end)),
    )
}

/// Return the first scalar of `line`.
///
/// Line 0 starts at scalar 0.
/// Each newline starts the next line.
/// When `line` is the line count, the result is the scalar count.
fn line_scalar(text: &str, line: u32) -> usize {
    let mut newlines = 0u32;
    let mut count = 0usize;
    for ch in text.chars() {
        if newlines == line {
            return count;
        }
        count += 1;
        if ch == '\n' {
            newlines += 1;
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
        assert_eq!(view.folds, vec![CharRange::new(Offset(0), Offset(6))]);
    }
}
