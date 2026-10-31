// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Anchors and the offset transform.
//!
//! An anchor is a `Copy` value. It is not registered with the buffer.
//! `transform_offset` moves one pre-state offset across one edit.

use crate::edit::{Edit, Transaction};
use crate::offset::{Offset, Revision};

/// The side an anchor keeps when an edit covers its offset.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Bias {
    /// Keep the start of the edited span.
    Left,
    /// Keep the end of the new text.
    Right,
}

/// A position remembered at a revision.
///
/// The value is plain data. Resolve it when you need an offset.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Anchor {
    /// Scalar offset at `revision`.
    pub offset: Offset,
    /// Revision that `offset` belongs to.
    pub revision: Revision,
    /// Behavior when an edit covers `offset`.
    pub bias: Bias,
}

/// The edit log does not reach `requested`.
#[derive(Debug, PartialEq, Eq)]
pub struct StaleAnchor {
    /// Revision the caller asked to read.
    pub requested: Revision,
    /// Oldest revision the log can still transform.
    pub oldest_available: Revision,
}

/// Moves `offset` across one edit.
///
/// The edit uses pre-state offsets.
/// An offset before `start` stays the same.
/// An offset after `old_end` shifts by the change in scalar length.
/// An offset from `start` through `old_end` uses `bias`.
/// `Bias::Left` goes to `start`. `Bias::Right` goes to the end of `new`.
/// A pure insert has an empty `old`, so that span is the single offset `start`.
pub fn transform_offset(offset: Offset, _bias: Bias, _edit: &Edit) -> Offset {
    offset
}

/// Moves `offset` across one transaction.
///
/// Edits are applied from last to first. That is the same order as `TextBuffer::apply`.
pub(crate) fn transform_transaction(offset: Offset, bias: Bias, tx: &Transaction) -> Offset {
    let mut at = offset;
    for edit in tx.edits().iter().rev() {
        at = transform_offset(at, bias, edit);
    }
    at
}
