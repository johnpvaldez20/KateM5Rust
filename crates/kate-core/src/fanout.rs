// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! One transaction for every selection.
//!
//! `fan_out` asks for one edit per selection.
//! The edits use pre-state offsets.
//! Overlapping edits return [`TransactionError`].

use kate_text::{CharRange, Edit, Offset, TextBuffer, Transaction, TransactionError};

use crate::movement::{grapheme_after, grapheme_before};
use crate::selection::{Selection, Selections};

/// Build one transaction from one edit per selection.
///
/// `f` may return `None` when that selection does not edit.
/// The returned set is `Selections::transformed` of the transaction.
pub fn fan_out(
    buf: &TextBuffer,
    sel: &Selections,
    mut f: impl FnMut(&TextBuffer, Selection) -> Option<Edit>,
) -> Result<(Transaction, Selections), TransactionError> {
    let sel = clamp_set(buf, sel);
    let mut edits = Vec::new();
    for selection in sel.iter().copied() {
        if let Some(edit) = f(buf, selection) {
            edits.push(edit);
        }
    }
    let tx = Transaction::new(edits)?;
    let after = sel.transformed(&tx);
    Ok((tx, after))
}

fn clamp_set(buf: &TextBuffer, sel: &Selections) -> Selections {
    let len = buf.len().0;
    let mut clamped = sel.clone();
    clamped.map(|selection| Selection {
        anchor: Offset(selection.anchor.0.min(len)),
        head: Offset(selection.head.0.min(len)),
        goal_col: selection.goal_col,
    });
    clamped
}

/// Replace each selection with `text`.
///
/// The returned set is a caret at the end of each insert.
/// One caret and many carets use this same function.
pub fn insert_text(
    buf: &TextBuffer,
    sel: &Selections,
    text: &str,
) -> Result<(Transaction, Selections), TransactionError> {
    let (tx, mut after) = fan_out(buf, sel, |buffer, selection| {
        Some(replace_span(buffer, selection.min(), selection.max(), text))
    })?;
    after.map(|selection| Selection::caret(selection.head));
    Ok((tx, after))
}

/// Delete one grapheme before a caret, or delete a selection.
pub fn delete_backward(
    buf: &TextBuffer,
    sel: &Selections,
) -> Result<(Transaction, Selections), TransactionError> {
    delete_with(buf, sel, false)
}

/// Delete one grapheme after a caret, or delete a selection.
pub fn delete_forward(
    buf: &TextBuffer,
    sel: &Selections,
) -> Result<(Transaction, Selections), TransactionError> {
    delete_with(buf, sel, true)
}

fn delete_with(
    buf: &TextBuffer,
    sel: &Selections,
    forward: bool,
) -> Result<(Transaction, Selections), TransactionError> {
    let (tx, mut after) = fan_out(buf, sel, |buffer, selection| {
        delete_edit(buffer, selection, forward)
    })?;
    after.map(|selection| Selection {
        anchor: selection.anchor,
        head: selection.head,
        goal_col: None,
    });
    Ok((tx, after))
}

fn delete_edit(buf: &TextBuffer, selection: Selection, forward: bool) -> Option<Edit> {
    let (start, end) = deletion_bounds(buf, selection, forward);
    let edit = replace_span(buf, start, end, "");
    if edit.old.is_empty() {
        None
    } else {
        Some(edit)
    }
}

fn deletion_bounds(buf: &TextBuffer, selection: Selection, forward: bool) -> (Offset, Offset) {
    if !selection.is_caret() {
        return (selection.min(), selection.max());
    }
    let head = Offset(selection.head.0.min(buf.len().0));
    if forward {
        (head, grapheme_after(buf, head))
    } else {
        (grapheme_before(buf, head), head)
    }
}

fn replace_span(buf: &TextBuffer, start: Offset, end: Offset, new_text: &str) -> Edit {
    let len = buf.len().0;
    let start = start.0.min(end.0).min(len);
    let end = start.max(end.0.min(len));
    let old = if start == end {
        String::new()
    } else {
        buf.slice(CharRange::new(Offset(start), Offset(end)))
            .to_string()
    };
    Edit {
        start: Offset(start),
        old,
        new: new_text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{delete_backward, delete_forward, fan_out, insert_text};
    use crate::selection::{Selection, Selections};
    use kate_text::{Edit, Offset, TextBuffer, TransactionError};

    fn oracle_insert(text: &str, spots: &[usize], insert: &str) -> (String, Vec<usize>) {
        let mut spots = spots.to_vec();
        spots.sort_unstable();
        let mut chars: Vec<char> = text.chars().collect();
        let extra: Vec<char> = insert.chars().collect();
        let mut carets = Vec::with_capacity(spots.len());
        let mut shift = 0usize;
        for spot in spots {
            let at = spot + shift;
            for (offset, ch) in extra.iter().enumerate() {
                chars.insert(at + offset, *ch);
            }
            shift += extra.len();
            carets.push(at + extra.len());
        }
        (chars.into_iter().collect(), carets)
    }

    #[test]
    fn typing_with_many_cursors_is_one_transaction() {
        let text = "a".repeat(100);
        let spots: Vec<usize> = (0..40).map(|index| index * 2).collect();
        let primary = 7usize;
        let selections = spots
            .iter()
            .map(|spot| Selection::caret(Offset(*spot)))
            .collect();
        let sel = Selections::new(selections, primary);
        let mut buf = TextBuffer::from_str(&text);
        let (tx, after) = insert_text(&buf, &sel, "zz").expect("edits do not overlap");
        assert_eq!(tx.edits().len(), 40);
        let (expected_text, carets) = oracle_insert(&text, &spots, "zz");
        buf.apply(&tx).expect("edits match the buffer");
        assert_eq!(buf.text().to_string(), expected_text);
        assert_eq!(after.len(), 40);
        let heads: Vec<Offset> = after.iter().map(|selection| selection.head).collect();
        let expected_heads: Vec<Offset> = carets.into_iter().map(Offset).collect();
        assert_eq!(heads, expected_heads);
        assert!(after.iter().all(|selection| selection.is_caret()));
        assert_eq!(after.primary().head, expected_heads[primary]);

        let mut one = TextBuffer::from_str("hello");
        let single = Selections::single(Selection::caret(Offset(5)));
        let (tx, after) = insert_text(&one, &single, "zz").expect("one caret uses fan-out");
        assert_eq!(tx.edits().len(), 1);
        one.apply(&tx).expect("the edit matches");
        assert_eq!(one.text().to_string(), "hellozz");
        assert_eq!(after.len(), 1);
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(7));
    }

    #[test]
    fn delete_backward_removes_one_grapheme() {
        let mut buf = TextBuffer::from_str("e\u{301}x");
        let sel = Selections::single(Selection::caret(Offset(2)));
        let (tx, after) = delete_backward(&buf, &sel).expect("one grapheme delete");
        buf.apply(&tx).expect("the delete matches");
        assert_eq!(buf.text().to_string(), "x");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(0));

        let buf = TextBuffer::from_str("a");
        let sel = Selections::single(Selection::caret(Offset(0)));
        let (tx, after) = delete_backward(&buf, &sel).expect("no edit at the start");
        assert!(tx.is_empty());
        assert_eq!(after.primary().head, Offset(0));
    }

    #[test]
    fn delete_on_a_selection_removes_the_span() {
        let mut buf = TextBuffer::from_str("abcd");
        let sel = Selections::single(Selection {
            anchor: Offset(1),
            head: Offset(3),
            goal_col: Some(4),
        });
        let (tx, after) = delete_forward(&buf, &sel).expect("the span deletes");
        buf.apply(&tx).expect("the delete matches");
        assert_eq!(buf.text().to_string(), "ad");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(1));
        assert_eq!(after.primary().goal_col, None);
    }

    #[test]
    fn overlapping_edits_return_an_error() {
        let buf = TextBuffer::from_str("abcdef");
        let sel = Selections::new(
            vec![Selection::caret(Offset(0)), Selection::caret(Offset(3))],
            1,
        );
        let result = fan_out(&buf, &sel, |_, _| Some(Edit::delete(Offset(0), "abc")));
        assert!(matches!(result, Err(TransactionError::Overlap { .. })));
    }

    #[test]
    fn commands_do_not_panic_on_bad_offsets() {
        let mut buf = TextBuffer::from_str("ab");
        let sel = Selections::single(Selection {
            anchor: Offset(40),
            head: Offset(90),
            goal_col: None,
        });
        let (tx, after) = insert_text(&buf, &sel, "q").expect("clamped insert");
        buf.apply(&tx).expect("insert applies");
        assert_eq!(buf.text().to_string(), "abq");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(3));

        let buf = TextBuffer::from_str("ab");
        let (tx, after) = delete_forward(&buf, &sel).expect("clamped delete");
        assert!(tx.is_empty());
        assert_eq!(after.primary().head, Offset(2));
        assert!(after.primary().is_caret());
    }
}
