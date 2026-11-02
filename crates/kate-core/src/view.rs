// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! One view of a document.
//!
//! The view owns selections and scroll state.
//! It does not own the text.

use kate_text::CharRange;

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
    /// Folded ranges. Empty until a later packet fills them.
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
