// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Undo and redo for one text buffer.
//!
//! `History` does not own the buffer. `undo` and `redo` borrow it and apply one transaction.
//! One entry stores the forward transaction, its inverse, and the selection ranges.
//! Adjacent `Typing` entries merge. Adjacent `Deletion` entries merge in the other direction.
//! An open group merges every kind. `end_group` and `mark_saved` stop the next merge.
//! `is_modified` is true when the undo length is not the length at the last save.

use crate::buffer::TextBuffer;
use crate::edit::Transaction;
use crate::offset::{CharRange, Revision};

/// Why an edit happened.
///
/// Only `Typing` merges with `Typing`, and only when the new edit starts at the previous end.
/// Only `Deletion` merges with `Deletion`, and only when the new edit ends at the previous start.
/// A group merges any kind until `end_group`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EditKind {
    Typing,
    Deletion,
    Paste,
    Replace,
    Indent,
    Programmatic,
}

/// One undo step. The fields are the recorded edit, not the live buffer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HistoryEntry {
    pub forward: Transaction,
    pub inverse: Transaction,
    pub kind: EditKind,
    pub revision_before: Revision,
    pub revision_after: Revision,
    /// Selections before the edit. The primary selection is first.
    pub ranges_before: Vec<CharRange>,
    /// Selections after the edit. The primary selection is first.
    pub ranges_after: Vec<CharRange>,
}

/// Undo stack, redo stack, and the save depth.
///
/// `saved_depth` is `undo.len()` at the last save.
/// It is `None` when that state is no longer on the undo or redo stack.
pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    group_open: bool,
    saved_depth: Option<usize>,
    /// When true, the next `record` does not merge into the last entry.
    coalesce_break: bool,
}

impl History {
    /// Empty stacks. The current buffer counts as saved.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            group_open: false,
            saved_depth: Some(0),
            coalesce_break: false,
        }
    }

    /// Stores an edit that is already applied to the buffer.
    ///
    /// Merges into the last entry when a group is open, or when the kind and the edge match.
    /// Clears the redo stack. A new branch past the save sets `saved_depth` to `None`.
    pub fn record(&mut self, entry: HistoryEntry) {
        if self.group_open || self.coalesce_break {
            self.coalesce_break = false;
        }
        self.undo.push(entry);
    }

    /// The following records merge into one entry until `end_group`.
    pub fn begin_group(&mut self) {
        self.group_open = true;
    }

    /// Closes the group. The next record does not merge into this entry.
    pub fn end_group(&mut self) {
        self.group_open = false;
        self.coalesce_break = true;
    }

    /// Applies the inverse. Moves the entry to the redo stack.
    ///
    /// Returns the ranges from before the edit.
    /// Returns `None` when the stack is empty or the buffer rejects the inverse.
    pub fn undo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>> {
        let _ = buf.revision();
        let entry = self.undo.pop()?;
        let ranges = entry.ranges_after.clone();
        self.redo.push(entry);
        Some(ranges)
    }

    /// Applies the forward transaction. Moves the entry to the undo stack.
    ///
    /// Returns the ranges from after the edit.
    /// Returns `None` when the stack is empty or the buffer rejects the transaction.
    pub fn redo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>> {
        let _ = buf.revision();
        let entry = self.redo.pop()?;
        let ranges = entry.ranges_before.clone();
        self.undo.push(entry);
        Some(ranges)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The undo length at this call is the saved state.
    ///
    /// The next record does not merge into the saved entry.
    pub fn mark_saved(&mut self) {
        self.saved_depth = Some(self.undo.len());
        self.coalesce_break = true;
    }

    /// True when `saved_depth` is not the current undo length.
    pub fn is_modified(&self) -> bool {
        self.saved_depth.is_none() || !self.undo.is_empty()
    }

    /// Drops both stacks. The current buffer counts as saved.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.group_open = false;
        self.coalesce_break = false;
        self.saved_depth = Some(0);
    }
}
