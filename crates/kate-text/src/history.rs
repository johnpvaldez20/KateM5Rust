// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Undo and redo for one text buffer.
//!
//! `History` does not own the buffer. `undo` and `redo` borrow it and apply one transaction.
//! One entry stores the forward transaction, its inverse, and the selection ranges.
//! Adjacent `Typing` entries append in place. Adjacent `Deletion` entries append in place.
//! A single-edit replace grows when later inserts start at its caret.
//! `begin_group` starts a new entry. Later records in that group stay in a list.
//! The group builds one forward and inverse pair once, by a balanced merge.
//! Each pair is one linear sweep. A cluster inside that sweep is one forward pass
//! over the edits and the text it covers. The build is about K log K for K edits.
//! The group keeps that pair.
//! `is_modified` is true when the undo length is not the length at the last save.

use crate::buffer::TextBuffer;
use crate::edit::{Edit, Transaction};
use crate::offset::{CharRange, Offset, Revision};

fn scalar_len(text: &str) -> usize {
    text.chars().count()
}

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
    undo: Vec<Stored>,
    redo: Vec<Stored>,
    group_open: bool,
    saved_depth: Option<usize>,
    /// When true, the next `record` does not merge into the last entry.
    /// `begin_group`, `end_group`, and `mark_saved` set it.
    /// Undo or redo that lands on the saved depth sets it.
    /// A new entry clears it.
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
        self.forget_save_if_redo_holds_it();
        self.redo.clear();
        if self.can_merge(&entry) {
            self.merge_last(entry);
            return;
        }
        self.push_new(entry);
    }

    /// Opens a group. The next record starts a new entry.
    ///
    /// Later records join that entry until `end_group`.
    pub fn begin_group(&mut self) {
        self.group_open = true;
        self.coalesce_break = true;
    }

    /// Closes the group. The next record does not merge into this entry.
    ///
    /// Builds the group's forward and inverse pair once, when the list is not empty.
    pub fn end_group(&mut self) {
        self.group_open = false;
        self.coalesce_break = true;
        if let Some(last) = self.undo.last_mut() {
            let _ = last.ensure_group_cache();
        }
    }

    /// Applies the inverse. Moves the entry to the redo stack.
    ///
    /// Returns the ranges from before the edit.
    /// Returns `None` when the stack is empty or the buffer rejects the inverse.
    /// A rejected apply leaves the history and the buffer unchanged.
    pub fn undo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>> {
        let mut entry = self.undo.pop()?;
        let Some(inverse) = entry.prepared_inverse() else {
            self.undo.push(entry);
            return None;
        };
        if buf.apply(&inverse).is_err() {
            self.undo.push(entry);
            return None;
        }
        let ranges = entry.meta.ranges_before.clone();
        self.redo.push(entry);
        self.seal_at_saved_depth();
        Some(ranges)
    }

    /// Applies the forward transaction. Moves the entry to the undo stack.
    ///
    /// Returns the ranges from after the edit.
    /// Returns `None` when the stack is empty or the buffer rejects the transaction.
    /// A rejected apply leaves the history and the buffer unchanged.
    pub fn redo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>> {
        let mut entry = self.redo.pop()?;
        let Some(forward) = entry.prepared_forward() else {
            self.redo.push(entry);
            return None;
        };
        if buf.apply(&forward).is_err() {
            self.redo.push(entry);
            return None;
        }
        let ranges = entry.meta.ranges_after.clone();
        self.undo.push(entry);
        self.seal_at_saved_depth();
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
        self.saved_depth != Some(self.undo.len())
    }

    /// Drops both stacks. The current buffer counts as saved.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.group_open = false;
        self.coalesce_break = false;
        self.saved_depth = Some(0);
    }

    fn can_merge(&self, entry: &HistoryEntry) -> bool {
        let Some(last) = self.undo.last() else {
            return false;
        };
        if self.coalesce_break {
            return false;
        }
        self.group_open || same_kind_adjacent(last, entry)
    }

    fn merge_last(&mut self, entry: HistoryEntry) {
        if self.group_open {
            self.merge_into_group(entry);
            return;
        }
        if self.append_typing(&entry)
            || self.append_deletion(&entry)
            || self.append_to_fixed(&entry)
        {
            return;
        }
        self.merge_by_compose(entry);
    }

    fn merge_into_group(&mut self, entry: HistoryEntry) {
        let Some(last) = self.undo.pop() else {
            self.push_new(entry);
            return;
        };
        self.undo.push(append_group_part(last, entry));
    }

    fn append_typing(&mut self, entry: &HistoryEntry) -> bool {
        let Some(edit) = pure_insert(&entry.forward) else {
            return false;
        };
        let added = scalar_len(&edit.new);
        let Some(last) = self.undo.last_mut() else {
            return false;
        };
        let Body::Typing {
            start,
            text,
            len_chars,
        } = &mut last.body
        else {
            return false;
        };
        let Some(end) = start.checked_add(*len_chars) else {
            return false;
        };
        if edit.start.0 != end {
            return false;
        }
        let Some(next_len) = len_chars.checked_add(added) else {
            return false;
        };
        text.push_str(&edit.new);
        *len_chars = next_len;
        last.meta.revision_after = entry.revision_after;
        last.meta.ranges_after = entry.ranges_after.clone();
        true
    }

    fn append_deletion(&mut self, entry: &HistoryEntry) -> bool {
        let Some(edit) = pure_delete(&entry.forward) else {
            return false;
        };
        let Some(last) = self.undo.last_mut() else {
            return false;
        };
        let Body::Deletion { start, reversed } = &mut last.body else {
            return false;
        };
        if edit.old_end().0 != *start {
            return false;
        }
        push_reversed(reversed, &edit.old);
        *start = edit.start.0;
        last.meta.revision_after = entry.revision_after;
        last.meta.ranges_after = entry.ranges_after.clone();
        true
    }

    fn append_to_fixed(&mut self, entry: &HistoryEntry) -> bool {
        let Some((ins_start, ins_new)) = insert_data(&entry.forward) else {
            return false;
        };
        let Some(last) = self.undo.last_mut() else {
            return false;
        };
        if grow_text(last, ins_start, &ins_new, entry) {
            return true;
        }
        let Some((start, old, mut text)) = single_fixed_edit(&last.body) else {
            return false;
        };
        let len_chars = scalar_len(&text);
        let Some(end) = start.checked_add(len_chars) else {
            return false;
        };
        if ins_start != end {
            return false;
        }
        let added = scalar_len(&ins_new);
        let Some(next_len) = len_chars.checked_add(added) else {
            return false;
        };
        text.push_str(&ins_new);
        last.body = Body::Grow {
            start,
            old,
            text,
            len_chars: next_len,
        };
        last.meta.revision_after = entry.revision_after;
        last.meta.ranges_after = entry.ranges_after.clone();
        true
    }

    fn merge_by_compose(&mut self, entry: HistoryEntry) {
        let Some(last) = self.undo.pop() else {
            self.push_new(entry);
            return;
        };
        let Some(forward) = last.forward_tx() else {
            self.undo.push(last);
            self.push_new(entry);
            return;
        };
        let Some(forward) = compose::transactions(&forward, &entry.forward) else {
            self.undo.push(last);
            self.push_new(entry);
            return;
        };
        let Some(inverse) = checked_invert(&forward) else {
            self.undo.push(last);
            self.push_new(entry);
            return;
        };
        self.undo.push(Stored {
            meta: Meta {
                kind: last.meta.kind,
                revision_before: last.meta.revision_before,
                revision_after: entry.revision_after,
                ranges_before: last.meta.ranges_before,
                ranges_after: entry.ranges_after,
            },
            body: Body::Fixed { forward, inverse },
        });
    }

    fn push_new(&mut self, entry: HistoryEntry) {
        self.coalesce_break = false;
        self.undo.push(Stored::from_entry(entry));
    }

    fn forget_save_if_redo_holds_it(&mut self) {
        if self
            .saved_depth
            .is_some_and(|depth| depth > self.undo.len())
        {
            self.saved_depth = None;
        }
    }

    fn seal_at_saved_depth(&mut self) {
        if self.saved_depth == Some(self.undo.len()) {
            self.coalesce_break = true;
        }
    }
}

struct Meta {
    kind: EditKind,
    revision_before: Revision,
    revision_after: Revision,
    ranges_before: Vec<CharRange>,
    ranges_after: Vec<CharRange>,
}

enum Body {
    /// One insert. `len_chars` is the scalar count of `text`.
    Typing {
        start: usize,
        text: String,
        len_chars: usize,
    },
    /// Pure deletions. `reversed` stores each new backspace at the end.
    Deletion { start: usize, reversed: String },
    Fixed {
        forward: Transaction,
        inverse: Transaction,
    },
    /// One edit. Later inserts at the caret append to `text`.
    Grow {
        start: usize,
        old: String,
        text: String,
        len_chars: usize,
    },
    /// Edits recorded while a group is open.
    /// `cached` is the composed pair. Later parts clear it.
    Group {
        parts: Vec<HistoryEntry>,
        cached: Option<(Transaction, Transaction)>,
    },
}

struct Stored {
    meta: Meta,
    body: Body,
}

impl Stored {
    fn from_entry(entry: HistoryEntry) -> Self {
        if entry.kind == EditKind::Typing
            && let Some((start, text)) = insert_data(&entry.forward)
        {
            return Self::typing(entry, start, text);
        }
        if entry.kind == EditKind::Deletion
            && let Some((start, deleted)) = delete_data(&entry.forward)
        {
            return Self::deletion(entry, start, &deleted);
        }
        Self {
            meta: Meta::from_entry(&entry),
            body: Body::Fixed {
                forward: entry.forward,
                inverse: entry.inverse,
            },
        }
    }

    fn typing(entry: HistoryEntry, start: usize, text: String) -> Self {
        let len_chars = scalar_len(&text);
        Self {
            meta: Meta::from_entry(&entry),
            body: Body::Typing {
                start,
                text,
                len_chars,
            },
        }
    }

    fn deletion(entry: HistoryEntry, start: usize, deleted: &str) -> Self {
        let mut reversed = String::new();
        push_reversed(&mut reversed, deleted);
        Self {
            meta: Meta::from_entry(&entry),
            body: Body::Deletion { start, reversed },
        }
    }

    fn forward_tx(&self) -> Option<Transaction> {
        self.pair().map(|(forward, _)| forward)
    }

    fn prepared_forward(&mut self) -> Option<Transaction> {
        self.ensure_group_cache()?;
        self.forward_tx()
    }

    fn prepared_inverse(&mut self) -> Option<Transaction> {
        self.ensure_group_cache()?;
        self.inverse_tx()
    }

    fn inverse_tx(&self) -> Option<Transaction> {
        self.pair().map(|(_, inverse)| inverse)
    }

    fn ensure_group_cache(&mut self) -> Option<()> {
        let Body::Group {
            cached: None,
            parts,
        } = &self.body
        else {
            return Some(());
        };
        let pair = group_pair(parts)?;
        let Body::Group { cached, .. } = &mut self.body else {
            return Some(());
        };
        *cached = Some(pair);
        Some(())
    }

    fn pair(&self) -> Option<(Transaction, Transaction)> {
        match &self.body {
            Body::Typing { start, text, .. } => Some(insert_pair(*start, text)),
            Body::Deletion { start, reversed } => Some(delete_pair(*start, reversed)),
            Body::Fixed { forward, inverse } => Some((forward.clone(), inverse.clone())),
            Body::Grow {
                start, old, text, ..
            } => Some(replace_pair(*start, old, text)),
            Body::Group {
                cached: Some(pair), ..
            } => Some(pair.clone()),
            Body::Group { parts, .. } => group_pair(parts),
        }
    }

    fn into_entry(self) -> HistoryEntry {
        let (forward, inverse) = self.pair().unwrap_or_else(empty_pair);
        HistoryEntry {
            forward,
            inverse,
            kind: self.meta.kind,
            revision_before: self.meta.revision_before,
            revision_after: self.meta.revision_after,
            ranges_before: self.meta.ranges_before,
            ranges_after: self.meta.ranges_after,
        }
    }
}

impl Meta {
    fn from_entry(entry: &HistoryEntry) -> Self {
        Self {
            kind: entry.kind,
            revision_before: entry.revision_before,
            revision_after: entry.revision_after,
            ranges_before: entry.ranges_before.clone(),
            ranges_after: entry.ranges_after.clone(),
        }
    }
}

fn same_kind_adjacent(last: &Stored, entry: &HistoryEntry) -> bool {
    if last.meta.kind != entry.kind {
        return false;
    }
    match entry.kind {
        EditKind::Typing => typing_touch(last, entry),
        EditKind::Deletion => deletion_touch(last, entry),
        EditKind::Paste | EditKind::Replace | EditKind::Indent | EditKind::Programmatic => false,
    }
}

fn typing_touch(last: &Stored, entry: &HistoryEntry) -> bool {
    let Some(next) = only_edit(&entry.forward) else {
        return false;
    };
    let Some(end) = typing_end(last) else {
        return false;
    };
    next.start.0 == end
}

fn typing_end(last: &Stored) -> Option<usize> {
    match &last.body {
        Body::Typing {
            start, len_chars, ..
        } => start.checked_add(*len_chars),
        Body::Fixed { forward, .. } => post_caret(forward),
        Body::Grow {
            start, len_chars, ..
        } => start.checked_add(*len_chars),
        Body::Deletion { .. } | Body::Group { .. } => None,
    }
}

fn deletion_touch(last: &Stored, entry: &HistoryEntry) -> bool {
    let Some(next) = only_edit(&entry.forward) else {
        return false;
    };
    if !next.new.is_empty() {
        return false;
    }
    let Some(edge) = deletion_start(last) else {
        return false;
    };
    next.old_end().0 == edge
}

fn deletion_start(last: &Stored) -> Option<usize> {
    match &last.body {
        Body::Deletion { start, .. } => Some(*start),
        Body::Fixed { forward, .. } => deletion_edge(forward).map(|offset| offset.0),
        Body::Typing { .. } | Body::Grow { .. } | Body::Group { .. } => None,
    }
}

fn append_group_part(mut last: Stored, entry: HistoryEntry) -> Stored {
    last.meta.revision_after = entry.revision_after;
    last.meta.ranges_after = entry.ranges_after.clone();
    if let Body::Group { parts, cached } = &mut last.body {
        parts.push(entry);
        *cached = None;
        return last;
    }
    let prev = last.into_entry();
    group_from(prev, entry)
}

fn group_from(prev: HistoryEntry, entry: HistoryEntry) -> Stored {
    let meta = Meta {
        kind: prev.kind,
        revision_before: prev.revision_before,
        revision_after: entry.revision_after,
        ranges_before: prev.ranges_before.clone(),
        ranges_after: entry.ranges_after.clone(),
    };
    Stored {
        meta,
        body: Body::Group {
            parts: vec![prev, entry],
            cached: None,
        },
    }
}

fn grow_text(last: &mut Stored, ins_start: usize, ins_new: &str, entry: &HistoryEntry) -> bool {
    let Body::Grow {
        start,
        text,
        len_chars,
        ..
    } = &mut last.body
    else {
        return false;
    };
    let Some(end) = start.checked_add(*len_chars) else {
        return false;
    };
    if ins_start != end {
        return false;
    }
    let Some(next_len) = len_chars.checked_add(scalar_len(ins_new)) else {
        return false;
    };
    text.push_str(ins_new);
    *len_chars = next_len;
    last.meta.revision_after = entry.revision_after;
    last.meta.ranges_after = entry.ranges_after.clone();
    true
}

fn single_fixed_edit(body: &Body) -> Option<(usize, String, String)> {
    let Body::Fixed { forward, .. } = body else {
        return None;
    };
    let edit = only_edit(forward)?;
    Some((edit.start.0, edit.old.clone(), edit.new.clone()))
}

fn replace_pair(start: usize, old: &str, new_text: &str) -> (Transaction, Transaction) {
    let forward = Transaction::single(Edit {
        start: Offset(start),
        old: old.to_string(),
        new: new_text.to_string(),
    });
    let inverse = Transaction::single(Edit {
        start: Offset(start),
        old: new_text.to_string(),
        new: old.to_string(),
    });
    (forward, inverse)
}

fn insert_pair(start: usize, text: &str) -> (Transaction, Transaction) {
    let forward = Transaction::single(Edit::insert(Offset(start), text.to_string()));
    let inverse = Transaction::single(Edit::delete(Offset(start), text.to_string()));
    (forward, inverse)
}

fn delete_pair(start: usize, reversed: &str) -> (Transaction, Transaction) {
    let text: String = reversed.chars().rev().collect();
    let forward = Transaction::single(Edit::delete(Offset(start), text.clone()));
    let inverse = Transaction::single(Edit::insert(Offset(start), text));
    (forward, inverse)
}

fn empty_pair() -> (Transaction, Transaction) {
    let empty = Transaction::new(Vec::new())
        .unwrap_or_else(|_| Transaction::single(Edit::insert(Offset(0), "")));
    (empty.clone(), empty)
}

fn insert_data(tx: &Transaction) -> Option<(usize, String)> {
    let edit = pure_insert(tx)?;
    Some((edit.start.0, edit.new.clone()))
}

fn delete_data(tx: &Transaction) -> Option<(usize, String)> {
    let edit = pure_delete(tx)?;
    Some((edit.start.0, edit.old.clone()))
}

fn group_pair(parts: &[HistoryEntry]) -> Option<(Transaction, Transaction)> {
    if let Some(pair) = fuse_inserts(parts) {
        return Some(pair);
    }
    if let Some(pair) = fuse_backspaces(parts) {
        return Some(pair);
    }
    compose_all(parts)
}

fn fuse_inserts(parts: &[HistoryEntry]) -> Option<(Transaction, Transaction)> {
    let mut parts = parts.iter();
    let first = parts.next()?;
    let edit = pure_insert(&first.forward)?;
    let start = edit.start.0;
    let mut text = edit.new.clone();
    let mut cursor = start.checked_add(scalar_len(&text))?;
    for part in parts {
        let edit = pure_insert(&part.forward)?;
        if edit.start.0 != cursor {
            return None;
        }
        text.push_str(&edit.new);
        cursor = cursor.checked_add(scalar_len(&edit.new))?;
    }
    Some(insert_pair(start, &text))
}

fn fuse_backspaces(parts: &[HistoryEntry]) -> Option<(Transaction, Transaction)> {
    let mut parts = parts.iter();
    let first = parts.next()?;
    let edit = pure_delete(&first.forward)?;
    let mut start = edit.start.0;
    let mut reversed = String::new();
    push_reversed(&mut reversed, &edit.old);
    for part in parts {
        let edit = pure_delete(&part.forward)?;
        if edit.old_end().0 != start {
            return None;
        }
        push_reversed(&mut reversed, &edit.old);
        start = edit.start.0;
    }
    Some(delete_pair(start, &reversed))
}

fn compose_all(parts: &[HistoryEntry]) -> Option<(Transaction, Transaction)> {
    let forward = compose_tree(parts)?;
    let inverse = checked_invert(&forward)?;
    Some((forward, inverse))
}

/// Pairwise merge. Each level walks the edits and their text once, so the cost is about K log K.
fn compose_tree(parts: &[HistoryEntry]) -> Option<Transaction> {
    let mut level: Vec<Transaction> = parts.iter().map(|part| part.forward.clone()).collect();
    if level.is_empty() {
        return None;
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut drain = level.into_iter();
        while let Some(left) = drain.next() {
            match drain.next() {
                Some(right) => next.push(compose::transactions(&left, &right)?),
                None => next.push(left),
            }
        }
        level = next;
    }
    level.pop()
}

fn pure_insert(tx: &Transaction) -> Option<&Edit> {
    let edit = only_edit(tx)?;
    if edit.old.is_empty() && !edit.new.is_empty() {
        Some(edit)
    } else {
        None
    }
}

fn pure_delete(tx: &Transaction) -> Option<&Edit> {
    let edit = only_edit(tx)?;
    if edit.new.is_empty() && !edit.old.is_empty() {
        Some(edit)
    } else {
        None
    }
}

fn push_reversed(reversed: &mut String, deleted: &str) {
    for ch in deleted.chars().rev() {
        reversed.push(ch);
    }
}

fn only_edit(tx: &Transaction) -> Option<&Edit> {
    let edits = tx.edits();
    if edits.len() == 1 {
        Some(&edits[0])
    } else {
        None
    }
}

/// Post-state offset just after the last edit's new text.
fn post_caret(tx: &Transaction) -> Option<usize> {
    let edits = tx.edits();
    let last = edits.last()?;
    let mut shift = 0isize;
    for edit in &edits[..edits.len() - 1] {
        shift = shift.checked_add(edit.delta())?;
    }
    let start = isize::try_from(last.start.0).ok()?.checked_add(shift)?;
    let new_len = isize::try_from(scalar_len(&last.new)).ok()?;
    let end = start.checked_add(new_len)?;
    usize::try_from(end).ok()
}

/// Earliest pre-state start of a pure deletion. `None` when any edit inserts text.
fn deletion_edge(tx: &Transaction) -> Option<Offset> {
    let edits = tx.edits();
    if edits.is_empty() {
        return None;
    }
    let mut edge: Option<Offset> = None;
    for edit in edits {
        if !edit.new.is_empty() {
            return None;
        }
        edge = Some(match edge {
            Some(current) => current.min(edit.start),
            None => edit.start,
        });
    }
    edge
}

/// Inverse of a transaction. `None` when the shift does not fit in `usize`.
fn checked_invert(tx: &Transaction) -> Option<Transaction> {
    let mut shift: isize = 0;
    let mut edits = Vec::with_capacity(tx.edits().len());
    for edit in tx.edits() {
        let start_i = isize::try_from(edit.start.0).ok()?;
        let post = start_i.checked_add(shift)?;
        let start = usize::try_from(post).ok()?;
        edits.push(Edit {
            start: Offset(start),
            old: edit.new.clone(),
            new: edit.old.clone(),
        });
        shift = shift.checked_add(edit.delta())?;
    }
    Transaction::new(edits).ok()
}

/// Compose two transactions.
///
/// `second` uses offsets in the post-state of `first`.
/// The result uses offsets in the pre-state of `first`.
mod compose {
    use super::scalar_len;
    use crate::edit::{Edit, Transaction};
    use crate::offset::Offset;

    #[derive(Clone, Debug)]
    enum Piece {
        Gap {
            pre: usize,
            len: usize,
        },
        Put {
            pre: usize,
            old: String,
            text: String,
        },
        Tail {
            pre: usize,
        },
    }

    struct Span {
        index: usize,
        post: usize,
        /// `None` is the unbounded tail. `Some(0)` is a zero-width put.
        vis: Option<usize>,
    }

    struct Loc {
        index: usize,
        into: usize,
    }

    pub(super) fn transactions(first: &Transaction, second: &Transaction) -> Option<Transaction> {
        if second.edits().is_empty() {
            return Some(first.clone());
        }
        if first.edits().is_empty() {
            return Some(second.clone());
        }
        let pieces = pieces_from(first)?;
        let spans = spans(&pieces);
        let swept = sweep(&pieces, &spans, second.edits())?;
        transaction_from(&swept)
    }

    /// One forward pass over `pieces` and `edits`.
    ///
    /// `edits` are sorted and do not overlap. They use the post-state of `pieces`.
    fn sweep(pieces: &[Piece], spans: &[Span], edits: &[Edit]) -> Option<Vec<Piece>> {
        let mut out = Vec::new();
        let mut pi = 0usize;
        let mut into = 0usize;
        let mut ei = 0usize;
        while ei < edits.len() {
            let start = edits[ei].start.0;
            copy_before(&mut out, pieces, spans, &mut pi, &mut into, start)?;
            if pi >= pieces.len() {
                return None;
            }
            let cursor = spans[pi].post + into;
            if edits[ei].old.is_empty() && cursor == start {
                let pre = pre_at(&pieces[pi], into);
                out.push(Piece::Put {
                    pre,
                    old: String::new(),
                    text: edits[ei].new.clone(),
                });
                ei += 1;
                continue;
            }
            let consumed = consume_cluster(&mut out, pieces, spans, &mut pi, &mut into, edits, ei)?;
            ei = consumed;
        }
        copy_before(&mut out, pieces, spans, &mut pi, &mut into, usize::MAX)?;
        Some(out)
    }

    fn copy_before(
        out: &mut Vec<Piece>,
        pieces: &[Piece],
        spans: &[Span],
        pi: &mut usize,
        into: &mut usize,
        target: usize,
    ) -> Option<()> {
        while *pi < pieces.len() {
            let vis = spans[*pi].vis;
            let post = spans[*pi].post + *into;
            if post >= target {
                return Some(());
            }
            let piece_end = match vis {
                None => usize::MAX,
                Some(len) => spans[*pi].post + len,
            };
            if matches!(&pieces[*pi], Piece::Put { .. })
                && *into == 0
                && spans[*pi].post < target
                && target < piece_end
            {
                return Some(());
            }
            if piece_end <= target {
                push_remainder(out, &pieces[*pi], *into);
                *pi += 1;
                *into = 0;
                continue;
            }
            let take = target - post;
            push_slice(out, &pieces[*pi], *into, take)?;
            *into += take;
            return Some(());
        }
        Some(())
    }

    fn consume_cluster(
        out: &mut Vec<Piece>,
        pieces: &[Piece],
        spans: &[Span],
        pi: &mut usize,
        into: &mut usize,
        edits: &[Edit],
        ei: usize,
    ) -> Option<usize> {
        let mut last = ei;
        let mut exp_end = edit_end(&edits[ei]);
        let mut hint = *pi;
        loop {
            let grown = expand_end(pieces, spans, &mut hint, exp_end);
            if grown > exp_end {
                exp_end = grown;
            }
            let mut next = last + 1;
            while next < edits.len() && edits[next].start.0 < exp_end {
                let end = edit_end(&edits[next]);
                if end > exp_end {
                    exp_end = end;
                }
                last = next;
                next += 1;
            }
            let grown = expand_end(pieces, spans, &mut hint, exp_end);
            if grown == exp_end {
                break;
            }
            exp_end = grown;
        }
        let pre = pre_at(&pieces[*pi], *into);
        let (old, text) = cluster_text(pieces, spans, *pi, *into, &edits[ei..=last], exp_end)?;
        if !(old.is_empty() && text.is_empty()) {
            out.push(Piece::Put { pre, old, text });
        }
        advance_to(pieces, spans, pi, into, exp_end)?;
        Some(last + 1)
    }

    fn expand_end(pieces: &[Piece], spans: &[Span], hint: &mut usize, end: usize) -> usize {
        let Some(loc) = locate_from(spans, end, *hint) else {
            return end;
        };
        *hint = loc.index;
        if loc.into == 0 {
            return end;
        }
        let Piece::Put { .. } = &pieces[loc.index] else {
            return end;
        };
        match spans[loc.index].vis {
            Some(len) => spans[loc.index].post + len,
            None => end,
        }
    }

    fn edit_end(edit: &Edit) -> usize {
        edit.start.0 + scalar_len(&edit.old)
    }

    fn cluster_text(
        pieces: &[Piece],
        spans: &[Span],
        pi: usize,
        into: usize,
        edits: &[Edit],
        exp_end: usize,
    ) -> Option<(String, String)> {
        let region_start = spans[pi].post + into;
        let visible = visible_text(pieces, spans, pi, into, exp_end, edits)?;
        let edited = apply_edits_to_chars(&visible, region_start, edits)?;
        let old = original_text(pieces, spans, pi, into, exp_end, edits)?;
        Some((old, edited))
    }

    fn visible_text(
        pieces: &[Piece],
        spans: &[Span],
        mut pi: usize,
        mut into: usize,
        exp_end: usize,
        edits: &[Edit],
    ) -> Option<String> {
        let mut out = String::new();
        let mut covered = OldChars::new(edits);
        while pi < pieces.len() && spans[pi].post + into < exp_end {
            let post = spans[pi].post + into;
            let piece_end = match spans[pi].vis {
                None => exp_end,
                Some(len) => spans[pi].post + len,
            };
            let stop = piece_end.min(exp_end);
            let take = stop - post;
            match &pieces[pi] {
                Piece::Put { text, .. } => append_chars(text, into, take, &mut out),
                Piece::Gap { .. } | Piece::Tail { .. } => covered.append(post, take, &mut out)?,
            }
            if stop == piece_end && spans[pi].vis.is_some() {
                pi += 1;
                into = 0;
            } else {
                break;
            }
        }
        Some(out)
    }

    fn original_text(
        pieces: &[Piece],
        spans: &[Span],
        mut pi: usize,
        mut into: usize,
        exp_end: usize,
        edits: &[Edit],
    ) -> Option<String> {
        let mut out = String::new();
        let mut covered = OldChars::new(edits);
        while pi < pieces.len() && spans[pi].post + into < exp_end {
            let post = spans[pi].post + into;
            let piece_end = match spans[pi].vis {
                None => exp_end,
                Some(len) => spans[pi].post + len,
            };
            let stop = piece_end.min(exp_end);
            match &pieces[pi] {
                Piece::Gap { .. } | Piece::Tail { .. } => {
                    covered.append(post, stop - post, &mut out)?;
                }
                Piece::Put { old, .. } => {
                    if into == 0 {
                        out.push_str(old);
                    }
                }
            }
            if stop == piece_end && spans[pi].vis.is_some() {
                pi += 1;
                into = 0;
            } else {
                break;
            }
        }
        Some(out)
    }

    /// Copies `take` scalars from `text`, starting at scalar `into`.
    ///
    /// The caller walks each string forward, so this runs once per slice.
    fn append_chars(text: &str, into: usize, take: usize, out: &mut String) {
        if take == 0 {
            return;
        }
        let Some(start) = byte_after_chars(text, into) else {
            return;
        };
        match byte_after_chars(&text[start..], take) {
            Some(end) => out.push_str(&text[start..start + end]),
            None => out.push_str(&text[start..]),
        }
    }

    fn apply_edits_to_chars(visible: &str, region: usize, edits: &[Edit]) -> Option<String> {
        let mut out = String::new();
        let mut rest = visible;
        let mut pos = 0usize;
        for edit in edits {
            let start = edit.start.0.checked_sub(region)?;
            if start < pos {
                return None;
            }
            let skip = byte_after_chars(rest, start - pos)?;
            let old_len = scalar_len(&edit.old);
            let old_bytes = byte_after_chars(&rest[skip..], old_len)?;
            let next = skip + old_bytes;
            out.push_str(&rest[..skip]);
            out.push_str(&edit.new);
            rest = &rest[next..];
            pos = start + old_len;
        }
        out.push_str(rest);
        Some(out)
    }

    /// Byte index after `count` scalars. `None` when `text` is shorter than `count`.
    fn byte_after_chars(text: &str, count: usize) -> Option<usize> {
        if count == 0 {
            return Some(0);
        }
        for (seen, (index, ch)) in text.char_indices().enumerate() {
            if seen + 1 == count {
                return Some(index + ch.len_utf8());
            }
        }
        None
    }

    /// Forward cursor over `edit.old` in post-state order.
    ///
    /// Each scalar of `old` is visited at most once.
    struct OldChars<'a> {
        edits: &'a [Edit],
        index: usize,
        into_chars: usize,
        into_bytes: usize,
        old_chars: usize,
        loaded: bool,
    }

    impl<'a> OldChars<'a> {
        fn new(edits: &'a [Edit]) -> Self {
            Self {
                edits,
                index: 0,
                into_chars: 0,
                into_bytes: 0,
                old_chars: 0,
                loaded: false,
            }
        }

        fn append(&mut self, post: usize, len: usize, out: &mut String) -> Option<()> {
            if len == 0 {
                return Some(());
            }
            let end = post.checked_add(len)?;
            let mut at = post;
            while at < end {
                let edit = self.edits.get(self.index)?;
                self.ensure_loaded();
                let lo = edit.start.0;
                let hi = lo + self.old_chars;
                if hi <= at {
                    self.bump();
                    continue;
                }
                if lo > at {
                    return None;
                }
                let skip = at - lo;
                if skip < self.into_chars {
                    return None;
                }
                self.advance(skip - self.into_chars, None);
                let take = hi.min(end) - at;
                self.advance(take, Some(out));
                at += take;
                if self.into_chars == self.old_chars {
                    self.bump();
                }
            }
            Some(())
        }

        fn ensure_loaded(&mut self) {
            if self.loaded {
                return;
            }
            self.old_chars = scalar_len(&self.edits[self.index].old);
            self.into_chars = 0;
            self.into_bytes = 0;
            self.loaded = true;
        }

        fn bump(&mut self) {
            self.index += 1;
            self.loaded = false;
        }

        fn advance(&mut self, count: usize, mut out: Option<&mut String>) {
            if count == 0 {
                return;
            }
            let start = self.into_bytes;
            let copied = {
                let text = &self.edits[self.index].old;
                match byte_after_chars(&text[start..], count) {
                    Some(end) => {
                        if let Some(dest) = out.as_mut() {
                            dest.push_str(&text[start..start + end]);
                        }
                        Some(start + end)
                    }
                    None => {
                        if let Some(dest) = out.as_mut() {
                            dest.push_str(&text[start..]);
                        }
                        None
                    }
                }
            };
            match copied {
                Some(byte) => {
                    self.into_bytes = byte;
                    self.into_chars += count;
                }
                None => {
                    self.into_bytes = self.edits[self.index].old.len();
                    self.into_chars = self.old_chars;
                }
            }
        }
    }

    fn advance_to(
        pieces: &[Piece],
        spans: &[Span],
        pi: &mut usize,
        into: &mut usize,
        target: usize,
    ) -> Option<()> {
        while *pi < pieces.len() && spans[*pi].post + *into < target {
            let piece_end = match spans[*pi].vis {
                None => {
                    *into += target - (spans[*pi].post + *into);
                    return Some(());
                }
                Some(len) => spans[*pi].post + len,
            };
            if piece_end <= target {
                *pi += 1;
                *into = 0;
            } else {
                *into += target - (spans[*pi].post + *into);
                return Some(());
            }
        }
        Some(())
    }

    fn locate_from(spans: &[Span], pos: usize, hint: usize) -> Option<Loc> {
        for span in spans.iter().skip(hint) {
            match span.vis {
                None => {
                    return Some(Loc {
                        index: span.index,
                        into: pos.checked_sub(span.post)?,
                    });
                }
                Some(0) => continue,
                Some(len) => {
                    let end = span.post + len;
                    if pos < end && pos >= span.post {
                        return Some(Loc {
                            index: span.index,
                            into: pos - span.post,
                        });
                    }
                    if pos < span.post {
                        return None;
                    }
                }
            }
        }
        None
    }

    fn pre_at(piece: &Piece, into: usize) -> usize {
        match piece {
            Piece::Gap { pre, .. } | Piece::Tail { pre } => pre + into,
            Piece::Put { pre, .. } => *pre,
        }
    }

    fn push_remainder(out: &mut Vec<Piece>, piece: &Piece, into: usize) {
        if into == 0 {
            out.push(piece.clone());
            return;
        }
        match piece {
            Piece::Gap { pre, len } => {
                if into < *len {
                    out.push(Piece::Gap {
                        pre: pre + into,
                        len: len - into,
                    });
                }
            }
            Piece::Tail { pre } => out.push(Piece::Tail { pre: pre + into }),
            Piece::Put { .. } => out.push(piece.clone()),
        }
    }

    fn push_slice(out: &mut Vec<Piece>, piece: &Piece, into: usize, take: usize) -> Option<()> {
        if take == 0 {
            return Some(());
        }
        match piece {
            Piece::Gap { pre, .. } => {
                out.push(Piece::Gap {
                    pre: pre + into,
                    len: take,
                });
                Some(())
            }
            Piece::Tail { pre } => {
                out.push(Piece::Gap {
                    pre: pre + into,
                    len: take,
                });
                Some(())
            }
            Piece::Put { .. } => None,
        }
    }

    fn pieces_from(tx: &Transaction) -> Option<Vec<Piece>> {
        let mut out = Vec::new();
        let mut pre = 0usize;
        for edit in tx.edits() {
            let start = edit.start.0;
            if start < pre {
                return None;
            }
            if start > pre {
                out.push(Piece::Gap {
                    pre,
                    len: start - pre,
                });
            }
            out.push(Piece::Put {
                pre: start,
                old: edit.old.clone(),
                text: edit.new.clone(),
            });
            pre = start.checked_add(scalar_len(&edit.old))?;
        }
        out.push(Piece::Tail { pre });
        Some(out)
    }

    fn transaction_from(pieces: &[Piece]) -> Option<Transaction> {
        let mut edits = Vec::new();
        for piece in pieces {
            let Piece::Put { pre, old, text } = piece else {
                continue;
            };
            if old.is_empty() && text.is_empty() {
                continue;
            }
            edits.push(Edit {
                start: Offset(*pre),
                old: old.clone(),
                new: text.clone(),
            });
        }
        Transaction::new(edits).ok()
    }

    fn spans(pieces: &[Piece]) -> Vec<Span> {
        let mut out = Vec::with_capacity(pieces.len());
        let mut cursor = 0usize;
        for (index, piece) in pieces.iter().enumerate() {
            let vis = match piece {
                Piece::Tail { .. } => None,
                Piece::Gap { len, .. } => Some(*len),
                Piece::Put { text, .. } => Some(scalar_len(text)),
            };
            out.push(Span {
                index,
                post: cursor,
                vis,
            });
            if let Some(len) = vis {
                cursor = cursor.saturating_add(len);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{EditKind, History, HistoryEntry};
    use crate::buffer::TextBuffer;
    use crate::edit::{Edit, Transaction};
    use crate::offset::{Offset, Revision};

    #[test]
    fn redo_onto_the_saved_depth_sets_the_seal() {
        let mut buf = TextBuffer::from_str("q");
        let mut history = History::new();
        record_insert(&mut history, &mut buf, 1, "Z");
        history.mark_saved();
        record_insert(&mut history, &mut buf, 2, "Y");
        history.undo(&mut buf).expect("undo Y");
        history.undo(&mut buf).expect("undo Z");
        history.coalesce_break = false;
        history.redo(&mut buf).expect("redo Z onto the saved depth");
        assert!(history.coalesce_break);
        assert!(!history.is_modified());
    }

    #[test]
    fn balanced_group_compose_matches_sequential_apply() {
        check_compose(
            "",
            &[
                Transaction::single(Edit::insert(Offset(0), "Q")),
                Transaction::single(Edit::insert(Offset(0), "a")),
            ],
        );
        check_pair(
            "ab",
            Edit::insert(Offset(0), "AB"),
            Edit::delete(Offset(1), "B"),
        );
        check_pair(
            "ab",
            Edit::insert(Offset(0), "HELLO"),
            Edit {
                start: Offset(2),
                old: "LL".to_string(),
                new: "YZ".to_string(),
            },
        );
        check_pair(
            "abcd",
            Edit::delete(Offset(1), "b"),
            Edit::insert(Offset(1), "Z"),
        );
        check_compose(
            "hello",
            &[
                Transaction::single(Edit::insert(Offset(0), "X")),
                Transaction::single(Edit::delete(Offset(1), "h")),
            ],
        );
        check_compose(
            "a",
            &[
                Transaction::single(Edit::insert(Offset(1), "Q")),
                Transaction::single(Edit::insert(Offset(0), "\t")),
            ],
        );
        check_compose(
            "abcd",
            &[
                Transaction::single(Edit::delete(Offset(1), "b")),
                Transaction::single(Edit::insert(Offset(1), "Z")),
            ],
        );
        let text = "abcdefghij";
        let mut parts = Vec::new();
        for index in [7usize, 1, 4, 9, 0] {
            let ch = text.chars().nth(index).expect("index is in range");
            parts.push(Transaction::single(Edit {
                start: Offset(index),
                old: ch.to_string(),
                new: "Z".to_string(),
            }));
        }
        check_compose(text, &parts);
        check_compose(
            "ab",
            &[
                Transaction::single(Edit::insert(Offset(0), "HELLO")),
                Transaction::single(Edit {
                    start: Offset(2),
                    old: "LL".to_string(),
                    new: "Z".to_string(),
                }),
            ],
        );
    }

    fn check_compose(initial: &str, parts: &[Transaction]) {
        let mut sequential = TextBuffer::from_str(initial);
        for part in parts {
            sequential.apply(part).expect("the part applies");
        }
        let entries: Vec<HistoryEntry> = parts.iter().cloned().map(entry_for).collect();
        let composed = super::compose_tree(&entries).expect("the parts compose");
        let mut once = TextBuffer::from_str(initial);
        once.apply(&composed).expect("the composed edit applies");
        assert_eq!(once.text().to_string(), sequential.text().to_string());
        let inverse = super::checked_invert(&composed).expect("the inverse exists");
        once.apply(&inverse).expect("the inverse applies");
        assert_eq!(once.text().to_string(), initial);
    }

    #[test]
    fn sweep_matches_sequential_apply_on_random_edits() {
        let mut state = 0x504B_5400_0000_0004u64;
        for _case in 0..300 {
            let width = 1 + (next_u32(&mut state) as usize % 30);
            let mut chars: Vec<char> = (0..width)
                .map(|index| char::from(b'a' + (index % 26) as u8))
                .collect();
            let initial: String = chars.iter().collect();
            let steps = 1 + (next_u32(&mut state) as usize % 48);
            let mut parts = Vec::with_capacity(steps);
            for _ in 0..steps {
                parts.push(Transaction::single(random_edit(&mut chars, &mut state)));
            }
            check_compose(&initial, &parts);
        }
    }

    fn random_edit(chars: &mut Vec<char>, state: &mut u64) -> Edit {
        let roll = next_u32(state) % 5;
        let len = chars.len();
        if len == 0 || roll == 0 {
            let at = if len == 0 {
                0
            } else {
                next_u32(state) as usize % (len + 1)
            };
            let ch = char::from(b'A' + (next_u32(state) % 26) as u8);
            chars.insert(at, ch);
            return Edit::insert(Offset(at), ch.to_string());
        }
        if roll == 1 {
            let at = next_u32(state) as usize % (len + 1);
            let text = if next_u32(state).is_multiple_of(2) {
                "AB"
            } else {
                "é"
            };
            for (offset, ch) in text.chars().enumerate() {
                chars.insert(at + offset, ch);
            }
            return Edit::insert(Offset(at), text);
        }
        let start = next_u32(state) as usize % len;
        let max_len = (len - start).min(3);
        let take = 1 + (next_u32(state) as usize % max_len);
        let old: String = chars[start..start + take].iter().collect();
        let new = match roll {
            2 => String::new(),
            3 => "Z".to_string(),
            _ => "YZ".to_string(),
        };
        chars.splice(start..start + take, new.chars());
        Edit {
            start: Offset(start),
            old,
            new,
        }
    }

    fn next_u32(state: &mut u64) -> u32 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (*state >> 32) as u32
    }

    fn check_pair(initial: &str, first: Edit, second: Edit) {
        let left = Transaction::single(first);
        let right = Transaction::single(second);
        check_compose(initial, &[left, right]);
    }

    fn entry_for(tx: Transaction) -> HistoryEntry {
        let inverse = tx.invert();
        HistoryEntry {
            forward: tx,
            inverse,
            kind: EditKind::Programmatic,
            revision_before: Revision(0),
            revision_after: Revision(1),
            ranges_before: Vec::new(),
            ranges_after: Vec::new(),
        }
    }

    fn record_insert(history: &mut History, buf: &mut TextBuffer, at: usize, text: &str) {
        let tx = Transaction::single(Edit::insert(Offset(at), text));
        let inverse = tx.invert();
        let revision_before = buf.revision();
        let revision_after = buf.apply(&tx).expect("the insert applies");
        history.record(HistoryEntry {
            forward: tx,
            inverse,
            kind: EditKind::Paste,
            revision_before,
            revision_after,
            ranges_before: Vec::new(),
            ranges_after: Vec::new(),
        });
    }
}
