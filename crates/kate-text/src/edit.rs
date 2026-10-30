// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Edits and transactions.
//!
//! One edit replaces `old` with `new` at a start offset.
//! Every edit in a transaction refers to the same pre-state.
//! `new` sorts the edits and rejects an overlap.

use crate::offset::Offset;

fn scalar_len(text: &str) -> usize {
    text.chars().count()
}

/// One replacement against the pre-transaction text. Insert: `old` empty. Delete: `new` empty.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edit {
    pub start: Offset,
    pub old: String,
    pub new: String,
}

impl Edit {
    /// Builds an insert. `old` is empty.
    pub fn insert(at: Offset, text: impl Into<String>) -> Self {
        Self {
            start: at,
            old: String::new(),
            new: text.into(),
        }
    }

    /// Builds a delete. `new` is empty.
    pub fn delete(at: Offset, old: impl Into<String>) -> Self {
        Self {
            start: at,
            old: old.into(),
            new: String::new(),
        }
    }

    /// Returns the offset after `old`.
    pub fn old_end(&self) -> Offset {
        Offset(self.start.0 + scalar_len(&self.old))
    }

    /// Returns `start` plus the scalar count of `new`.
    pub fn new_end(&self) -> Offset {
        Offset(self.start.0 + scalar_len(&self.new))
    }

    /// Returns the change in scalar count. The count of `new` minus the count of `old`.
    pub fn delta(&self) -> isize {
        let new_len = scalar_len(&self.new) as isize;
        let old_len = scalar_len(&self.old) as isize;
        new_len - old_len
    }

    /// Swaps `old` and `new`. `start` stays the same.
    pub fn invert(&self) -> Edit {
        Self {
            start: self.start,
            old: self.new.clone(),
            new: self.old.clone(),
        }
    }
}

/// Edits sorted by `start`, pairwise non-overlapping, all relative to the same pre-state.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Transaction {
    edits: Vec<Edit>,
}

impl Transaction {
    /// Sorts the edits by start. Returns an error when two edits overlap.
    pub fn new(edits: Vec<Edit>) -> Result<Self, TransactionError> {
        let mut indexed: Vec<(usize, Edit)> = edits.into_iter().enumerate().collect();
        indexed.sort_by(|left, right| {
            left.1
                .start
                .cmp(&right.1.start)
                .then(rank(left).cmp(&rank(right)))
                .then(left.0.cmp(&right.0))
        });
        for pair in indexed.windows(2) {
            if overlaps(&pair[0].1, &pair[1].1) {
                return Err(TransactionError::Overlap {
                    first: pair[0].0,
                    second: pair[1].0,
                });
            }
        }
        Ok(Self {
            edits: indexed.into_iter().map(|(_, edit)| edit).collect(),
        })
    }

    /// Builds a transaction that holds one edit.
    pub fn single(edit: Edit) -> Self {
        Self { edits: vec![edit] }
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// Builds the inverse transaction. The result applies to the post-state.
    pub fn invert(&self) -> Transaction {
        let mut shift: isize = 0;
        let mut edits = Vec::with_capacity(self.edits.len());
        for edit in &self.edits {
            let post = edit.start.0 as isize + shift;
            let start = usize::try_from(post).expect("inverse start is negative");
            edits.push(Edit {
                start: Offset(start),
                old: edit.new.clone(),
                new: edit.old.clone(),
            });
            shift = shift
                .checked_add(edit.delta())
                .expect("delta sum fits in isize");
        }
        Transaction::new(edits).expect("inverse of a valid transaction is valid")
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// A zero-width insert sorts before a replacement at the same start.
/// `apply` walks back to front, so the replacement runs first.
fn rank(indexed: &(usize, Edit)) -> u8 {
    if indexed.1.old.is_empty() { 0 } else { 1 }
}

/// Half-open spans overlap when they share a scalar.
/// An insert at `start` does not share a scalar with the span that begins there.
fn overlaps(left: &Edit, right: &Edit) -> bool {
    let left_end = left.old_end().0;
    let right_end = right.old_end().0;
    left.start.0 < right_end && right.start.0 < left_end
}

/// `Transaction::new` rejected the edit list.
#[derive(Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// `first` and `second` are indices into the input list.
    /// `first` is the edit with the earlier start.
    Overlap { first: usize, second: usize },
}

#[cfg(test)]
mod tests {
    use super::{Edit, Transaction, TransactionError};
    use crate::offset::Offset;

    #[test]
    fn overlapping_edits_are_rejected_at_construction() {
        let overlap = Transaction::new(vec![
            Edit::delete(Offset(0), "abc"),
            Edit::delete(Offset(2), "cde"),
        ]);
        assert_eq!(
            overlap,
            Err(TransactionError::Overlap {
                first: 0,
                second: 1
            })
        );

        let overlap_reversed = Transaction::new(vec![
            Edit::delete(Offset(2), "cde"),
            Edit::delete(Offset(0), "abc"),
        ]);
        assert_eq!(
            overlap_reversed,
            Err(TransactionError::Overlap {
                first: 1,
                second: 0
            })
        );

        let touching = Transaction::new(vec![
            Edit::delete(Offset(2), "cd"),
            Edit::delete(Offset(0), "ab"),
        ])
        .expect("adjacent edits are valid");
        assert_eq!(touching.edits()[0].start, Offset(0));
        assert_eq!(touching.edits()[1].start, Offset(2));

        let same_point = Transaction::new(vec![
            Edit::insert(Offset(1), "Y"),
            Edit::insert(Offset(1), "X"),
        ])
        .expect("inserts at one offset do not overlap");
        assert_eq!(same_point.edits()[0].new, "Y");
        assert_eq!(same_point.edits()[1].new, "X");

        let at_start = Transaction::new(vec![
            Edit::delete(Offset(0), "ab"),
            Edit::insert(Offset(0), "X"),
        ])
        .expect("an insert at the start of a span shares no scalar");
        assert_eq!(at_start.edits()[0].new, "X");
        assert_eq!(at_start.edits()[1].old, "ab");

        let inside = Transaction::new(vec![
            Edit::delete(Offset(0), "abc"),
            Edit::insert(Offset(1), "X"),
        ]);
        assert_eq!(
            inside,
            Err(TransactionError::Overlap {
                first: 0,
                second: 1
            })
        );

        let empty = Transaction::new(vec![]).expect("an empty list is valid");
        assert!(empty.is_empty());
        let one = Transaction::single(Edit::insert(Offset(4), "q"));
        assert!(!one.is_empty());
        assert_eq!(one.edits().len(), 1);
    }

    #[test]
    fn scalar_counts_drive_span_and_inverse() {
        let insert = Edit::insert(Offset(2), "é");
        assert_eq!(insert.old, "");
        assert_eq!(insert.new, "é");
        assert_eq!(insert.old_end(), Offset(2));
        assert_eq!(insert.new_end(), Offset(3));
        assert_eq!(insert.delta(), 1);
        let inverse = insert.invert();
        assert_eq!(inverse.old, "é");
        assert_eq!(inverse.new, "");
        assert_eq!(inverse.start, Offset(2));

        let delete = Edit::delete(Offset(1), "ab");
        assert_eq!(delete.delta(), -2);
        assert_eq!(delete.old_end(), Offset(3));
        assert_eq!(delete.new_end(), Offset(1));
    }
}
