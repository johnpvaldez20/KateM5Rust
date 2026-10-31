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
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    group_open: bool,
    saved_depth: Option<usize>,
    /// When true, the next `record` does not merge into the last entry.
    /// `end_group` and `mark_saved` set it. A new entry clears it.
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
    /// A rejected apply leaves the history and the buffer unchanged.
    pub fn undo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>> {
        let entry = self.undo.pop()?;
        if buf.apply(&entry.inverse).is_err() {
            self.undo.push(entry);
            return None;
        }
        let ranges = entry.ranges_before.clone();
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
        let entry = self.redo.pop()?;
        if buf.apply(&entry.forward).is_err() {
            self.redo.push(entry);
            return None;
        }
        let ranges = entry.ranges_after.clone();
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
        let forward = self
            .undo
            .last()
            .and_then(|last| compose::transactions(&last.forward, &entry.forward));
        let Some(forward) = forward else {
            self.push_new(entry);
            return;
        };
        let Some(inverse) = checked_invert(&forward) else {
            self.push_new(entry);
            return;
        };
        let Some(last) = self.undo.last_mut() else {
            self.push_new(entry);
            return;
        };
        last.forward = forward;
        last.inverse = inverse;
        last.revision_after = entry.revision_after;
        last.ranges_after = entry.ranges_after;
    }

    fn push_new(&mut self, entry: HistoryEntry) {
        self.coalesce_break = false;
        self.undo.push(entry);
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

fn same_kind_adjacent(last: &HistoryEntry, entry: &HistoryEntry) -> bool {
    if last.kind != entry.kind {
        return false;
    }
    match entry.kind {
        EditKind::Typing => forward_touch(last, entry),
        EditKind::Deletion => backward_touch(last, entry),
        EditKind::Paste | EditKind::Replace | EditKind::Indent | EditKind::Programmatic => false,
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

fn forward_touch(last: &HistoryEntry, entry: &HistoryEntry) -> bool {
    let Some(end) = post_caret(&last.forward) else {
        return false;
    };
    let Some(next) = only_edit(&entry.forward) else {
        return false;
    };
    next.start.0 == end
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

fn backward_touch(last: &HistoryEntry, entry: &HistoryEntry) -> bool {
    let Some(edge) = deletion_edge(&last.forward) else {
        return false;
    };
    let Some(next) = only_edit(&entry.forward) else {
        return false;
    };
    next.new.is_empty() && next.old_end() == edge
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

    enum SplitKind {
        Put,
        Gap { pre: usize, len: usize },
        Tail { pre: usize },
    }

    struct Expansion {
        start: usize,
        end: usize,
        prefix: String,
        suffix: String,
    }

    pub(super) fn transactions(first: &Transaction, second: &Transaction) -> Option<Transaction> {
        let mut pieces = pieces_from(first)?;
        for edit in second.edits().iter().rev() {
            pieces = apply_edit(pieces, edit)?;
        }
        transaction_from(&pieces)
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

    fn apply_edit(pieces: Vec<Piece>, edit: &Edit) -> Option<Vec<Piece>> {
        if edit.old.is_empty() {
            return apply_insert(pieces, edit.start.0, &edit.new);
        }
        apply_replace(pieces, edit)
    }

    fn apply_insert(mut pieces: Vec<Piece>, at: usize, text: &str) -> Option<Vec<Piece>> {
        if text.is_empty() {
            return Some(pieces);
        }
        let located = locate(&spans(&pieces), at)?;
        if located.into == 0 {
            let pre = pre_of(&pieces[located.index]);
            pieces.insert(
                located.index,
                Piece::Put {
                    pre,
                    old: String::new(),
                    text: text.to_string(),
                },
            );
            return Some(pieces);
        }
        if let Piece::Put { text: existing, .. } = &pieces[located.index] {
            let new_text = splice_chars(existing, located.into, text);
            if let Piece::Put { text, .. } = &mut pieces[located.index] {
                *text = new_text;
            }
            return Some(pieces);
        }
        split_gap_or_tail(&mut pieces, at)?;
        apply_insert(pieces, at, text)
    }

    fn apply_replace(mut pieces: Vec<Piece>, edit: &Edit) -> Option<Vec<Piece>> {
        let start = edit.start.0;
        let end = start.checked_add(scalar_len(&edit.old))?;
        let expansion = expand(&pieces, start, end)?;
        split_gap_or_tail(&mut pieces, expansion.start)?;
        split_gap_or_tail(&mut pieces, expansion.end)?;
        let spans = spans(&pieces);
        let covered = covered_indices(&spans, expansion.start, expansion.end);
        let (first, last) = contiguous(&covered)?;
        let new_text = join3(&expansion.prefix, &edit.new, &expansion.suffix);
        let old = composed_old(&pieces, &spans, first, last, start, end, &edit.old)?;
        let pre = pre_of(&pieces[first]);
        let piece = if old.is_empty() && new_text.is_empty() {
            None
        } else {
            Some(Piece::Put {
                pre,
                old,
                text: new_text,
            })
        };
        Some(splice(pieces, first, last, piece))
    }

    fn expand(pieces: &[Piece], start: usize, end: usize) -> Option<Expansion> {
        let spans = spans(pieces);
        let mut exp_start = start;
        let mut exp_end = end;
        let mut prefix = String::new();
        let mut suffix = String::new();
        absorb_start(pieces, &spans, start, &mut exp_start, &mut prefix)?;
        absorb_end(pieces, &spans, end, &mut exp_end, &mut suffix)?;
        if exp_end < exp_start {
            return None;
        }
        Some(Expansion {
            start: exp_start,
            end: exp_end,
            prefix,
            suffix,
        })
    }

    fn absorb_start(
        pieces: &[Piece],
        spans: &[Span],
        start: usize,
        exp_start: &mut usize,
        prefix: &mut String,
    ) -> Option<()> {
        let loc = locate(spans, start)?;
        if loc.into == 0 {
            return Some(());
        }
        let Piece::Put { text, .. } = &pieces[loc.index] else {
            return Some(());
        };
        *exp_start = spans.iter().find(|span| span.index == loc.index)?.post;
        *prefix = slice_chars(text, 0, loc.into);
        Some(())
    }

    fn absorb_end(
        pieces: &[Piece],
        spans: &[Span],
        end: usize,
        exp_end: &mut usize,
        suffix: &mut String,
    ) -> Option<()> {
        let loc = locate(spans, end)?;
        if loc.into == 0 {
            return Some(());
        }
        let Piece::Put { text, .. } = &pieces[loc.index] else {
            return Some(());
        };
        let post = spans.iter().find(|span| span.index == loc.index)?.post;
        let vis = scalar_len(text);
        *exp_end = post.checked_add(vis)?;
        *suffix = slice_chars(text, loc.into, vis);
        Some(())
    }

    fn composed_old(
        pieces: &[Piece],
        spans: &[Span],
        first: usize,
        last: usize,
        orig_start: usize,
        orig_end: usize,
        edit_old: &str,
    ) -> Option<String> {
        let mut pending = edit_old.chars();
        let mut old = String::new();
        for (index, piece) in pieces.iter().enumerate().take(last + 1).skip(first) {
            let span = spans.get(index)?;
            match piece {
                Piece::Put {
                    old: put_old, text, ..
                } => {
                    old.push_str(put_old);
                    let vis = scalar_len(text);
                    let post_end = span.post.checked_add(vis)?;
                    let lo = span.post.max(orig_start);
                    let hi = post_end.min(orig_end);
                    skip_chars(&mut pending, hi.saturating_sub(lo))?;
                }
                Piece::Gap { len, .. } => {
                    let take = take_chars(&mut pending, *len)?;
                    old.push_str(&take);
                }
                Piece::Tail { .. } => return None,
            }
        }
        if pending.next().is_some() {
            return None;
        }
        let pre = pre_of(&pieces[first]);
        let end_pre = pre_end_of(&pieces[last])?;
        if scalar_len(&old) != end_pre.checked_sub(pre)? {
            return None;
        }
        Some(old)
    }

    fn skip_chars(pending: &mut impl Iterator<Item = char>, count: usize) -> Option<()> {
        for _ in 0..count {
            pending.next()?;
        }
        Some(())
    }

    fn take_chars(pending: &mut impl Iterator<Item = char>, count: usize) -> Option<String> {
        let mut out = String::new();
        for _ in 0..count {
            out.push(pending.next()?);
        }
        Some(out)
    }

    fn covered_indices(spans: &[Span], start: usize, end: usize) -> Vec<usize> {
        spans
            .iter()
            .filter(|span| is_covered(span, start, end))
            .map(|span| span.index)
            .collect()
    }

    fn contiguous(covered: &[usize]) -> Option<(usize, usize)> {
        let first = *covered.first()?;
        let last = *covered.last()?;
        let width = last.checked_sub(first)?.checked_add(1)?;
        if width == covered.len() {
            Some((first, last))
        } else {
            None
        }
    }

    fn is_covered(span: &Span, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        match span.vis {
            None => span.post < end,
            Some(0) => start <= span.post && span.post < end,
            Some(len) => start < span.post.saturating_add(len) && span.post < end,
        }
    }

    fn split_gap_or_tail(pieces: &mut Vec<Piece>, pos: usize) -> Option<()> {
        let located = locate(&spans(pieces), pos)?;
        if located.into == 0 {
            return Some(());
        }
        let into = located.into;
        let index = located.index;
        let kind = match &pieces[index] {
            Piece::Put { .. } => SplitKind::Put,
            Piece::Gap { pre, len } => SplitKind::Gap {
                pre: *pre,
                len: *len,
            },
            Piece::Tail { pre } => SplitKind::Tail { pre: *pre },
        };
        match kind {
            SplitKind::Put => None,
            SplitKind::Gap { pre, len } => {
                if into >= len {
                    return None;
                }
                let right_pre = pre.checked_add(into)?;
                pieces[index] = Piece::Gap { pre, len: into };
                pieces.insert(
                    index + 1,
                    Piece::Gap {
                        pre: right_pre,
                        len: len - into,
                    },
                );
                Some(())
            }
            SplitKind::Tail { pre } => {
                let right_pre = pre.checked_add(into)?;
                pieces[index] = Piece::Gap { pre, len: into };
                pieces.insert(index + 1, Piece::Tail { pre: right_pre });
                Some(())
            }
        }
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

    fn locate(spans: &[Span], pos: usize) -> Option<Loc> {
        for span in spans {
            match span.vis {
                None => {
                    return Some(Loc {
                        index: span.index,
                        into: pos.checked_sub(span.post)?,
                    });
                }
                Some(0) => continue,
                Some(len) => {
                    let end = span.post.saturating_add(len);
                    if pos < end {
                        return Some(Loc {
                            index: span.index,
                            into: pos - span.post,
                        });
                    }
                }
            }
        }
        None
    }

    fn pre_of(piece: &Piece) -> usize {
        match piece {
            Piece::Gap { pre, .. } | Piece::Put { pre, .. } | Piece::Tail { pre } => *pre,
        }
    }

    fn pre_end_of(piece: &Piece) -> Option<usize> {
        match piece {
            Piece::Gap { pre, len } => pre.checked_add(*len),
            Piece::Put { pre, old, .. } => pre.checked_add(scalar_len(old)),
            Piece::Tail { .. } => None,
        }
    }

    fn splice(pieces: Vec<Piece>, first: usize, last: usize, piece: Option<Piece>) -> Vec<Piece> {
        let mut out = Vec::with_capacity(pieces.len());
        out.extend(pieces.iter().take(first).cloned());
        if let Some(piece) = piece {
            out.push(piece);
        }
        out.extend(pieces.into_iter().skip(last + 1));
        out
    }

    fn slice_chars(text: &str, start: usize, end: usize) -> String {
        text.chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    fn splice_chars(text: &str, at: usize, insert: &str) -> String {
        let mut out = String::new();
        let mut count = 0usize;
        for ch in text.chars() {
            if count == at {
                out.push_str(insert);
            }
            out.push(ch);
            count += 1;
        }
        if count == at {
            out.push_str(insert);
        }
        out
    }

    fn join3(left: &str, mid: &str, right: &str) -> String {
        let mut out = String::with_capacity(left.len() + mid.len() + right.len());
        out.push_str(left);
        out.push_str(mid);
        out.push_str(right);
        out
    }
}
