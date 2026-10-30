// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! The rope buffer.
//!
//! `apply` checks every edit, then changes the rope, then increases the revision by 1.
//! A failed apply leaves the rope and the revision unchanged.

use crate::edit::{Edit, Transaction};
use crate::offset::{CharRange, LineCol, Offset, Revision};
use ropey::{Rope, RopeSlice};

pub struct TextBuffer {
    text: Rope,
    revision: Revision,
}

impl TextBuffer {
    /// The packet defines `new` and does not define `Default`.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::from_str("")
    }

    /// The packet defines an infallible `from_str`. `FromStr` returns `Result`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            text: Rope::from_str(s),
            revision: Revision(0),
        }
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn snapshot(&self) -> Rope {
        self.text.clone()
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Returns the scalar count as an offset.
    pub fn len(&self) -> Offset {
        Offset(self.text.len_chars())
    }

    /// Returns the line count. A buffer always has a last line, so the count is at least 1.
    pub fn line_count(&self) -> u32 {
        u32::try_from(self.text.len_lines()).expect("line count fits in u32")
    }

    pub fn line_col(&self, at: Offset) -> LineCol {
        let at = at.0.min(self.text.len_chars());
        let line = self.text.char_to_line(at);
        let start = self.text.line_to_char(line);
        LineCol {
            line: u32::try_from(line).expect("line fits in u32"),
            col: u32::try_from(at - start).expect("column fits in u32"),
        }
    }

    /// Returns the offset for a line and a column.
    /// The column stops before the line break.
    /// A line past the last line uses the last line.
    pub fn offset(&self, at: LineCol) -> Offset {
        let last = self.text.len_lines() - 1;
        let line = (at.line as usize).min(last);
        let start = self.text.line_to_char(line);
        let mut end = self.text.line_to_char(line + 1);
        if end > start && self.text.char(end - 1) == '\n' {
            end -= 1;
        }
        let col = (at.col as usize).min(end - start);
        Offset(start + col)
    }

    pub fn slice(&self, range: CharRange) -> RopeSlice<'_> {
        self.text.slice(range.start().0..range.end().0)
    }

    /// Validates every edit against the current text, applies all, bumps revision by exactly 1.
    pub fn apply(&mut self, tx: &Transaction) -> Result<Revision, ApplyError> {
        validate(&self.text, tx)?;
        for edit in tx.edits().iter().rev() {
            apply_edit(&mut self.text, edit);
        }
        self.revision = Revision(self.revision.0 + 1);
        Ok(self.revision)
    }
}

fn validate(text: &Rope, tx: &Transaction) -> Result<(), ApplyError> {
    for (index, edit) in tx.edits().iter().enumerate() {
        check_edit(text, index, edit)?;
    }
    Ok(())
}

fn check_edit(text: &Rope, index: usize, edit: &Edit) -> Result<(), ApplyError> {
    let len = text.len_chars();
    let end = edit.old_end().0;
    if end > len {
        return Err(ApplyError::OutOfBounds {
            edit: index,
            end: Offset(end),
            len: Offset(len),
        });
    }
    let found = text.slice(edit.start.0..end).to_string();
    if found != edit.old {
        return Err(ApplyError::OldTextMismatch {
            edit: index,
            expected: edit.old.clone(),
            found,
        });
    }
    Ok(())
}

fn apply_edit(text: &mut Rope, edit: &Edit) {
    let start = edit.start.0;
    let end = edit.old_end().0;
    if start != end {
        text.remove(start..end);
    }
    if !edit.new.is_empty() {
        text.insert(start, &edit.new);
    }
}

/// `TextBuffer::apply` rejected a transaction.
#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// `edit` is an index into the transaction.
    /// `end` is that edit's `old_end`. `len` is the buffer length in scalars.
    OutOfBounds {
        edit: usize,
        end: Offset,
        len: Offset,
    },
    /// `found` is the buffer slice with the same scalar count as `expected`.
    OldTextMismatch {
        edit: usize,
        expected: String,
        found: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{ApplyError, TextBuffer};
    use crate::edit::{Edit, Transaction};
    use crate::offset::{CharRange, LineCol, Offset, Revision};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    /// Fixed seed so a local run and a CI run use the same cases.
    const PROPTEST_SEED: u64 = 0x504B_5400_0000_0002;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn arb_scalar() -> impl Strategy<Value = char> {
        prop_oneof![
            6 => prop::char::range('a', 'z'),
            1 => Just('\n'),
            1 => Just('\r'),
            1 => Just('é'),
            1 => Just('你'),
            1 => Just('\u{1F600}'),
        ]
    }

    fn arb_line_char() -> impl Strategy<Value = char> {
        prop_oneof![
            6 => prop::char::range('a', 'z'),
            1 => Just('\r'),
            1 => Just('é'),
            1 => Just('你'),
            1 => Just('\u{1F600}'),
        ]
    }

    fn arb_snippet() -> impl Strategy<Value = String> {
        prop::collection::vec(arb_scalar(), 0..8).prop_map(|chars| chars.into_iter().collect())
    }

    fn arb_text() -> impl Strategy<Value = String> {
        prop::collection::vec(arb_scalar(), 0..40).prop_map(|chars| chars.into_iter().collect())
    }

    fn arb_line() -> impl Strategy<Value = String> {
        prop::collection::vec(arb_line_char(), 0..12).prop_map(|chars| chars.into_iter().collect())
    }

    fn arb_multiline() -> impl Strategy<Value = String> {
        (
            arb_line(),
            arb_line(),
            prop::collection::vec(arb_line(), 0..3),
        )
            .prop_map(|(first, second, rest)| {
                let mut lines = vec![first, second];
                lines.extend(rest);
                lines.join("\n")
            })
    }

    /// Applies sorted edits on a `Vec<char>`. This oracle does not use the rope.
    fn text_after(text: &str, tx: &Transaction) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut edits = tx.edits().to_vec();
        // Stable sort keeps input order when two inserts share a start.
        edits.sort_by_key(|edit| edit.start);
        let mut out = Vec::new();
        let mut cursor = 0usize;
        for edit in &edits {
            let start = edit.start.0;
            let old_len = edit.old.chars().count();
            assert!(start >= cursor, "edits overlap or are unsorted");
            assert!(start + old_len <= chars.len(), "edit ends past the text");
            out.extend_from_slice(&chars[cursor..start]);
            let found: String = chars[start..start + old_len].iter().collect();
            assert_eq!(found, edit.old, "old text does not match the pre-state");
            out.extend(edit.new.chars());
            cursor = start + old_len;
        }
        out.extend_from_slice(&chars[cursor..]);
        out.into_iter().collect()
    }

    fn build_transaction(text: &str, parts: &[(usize, usize, String)]) -> Transaction {
        let chars: Vec<char> = text.chars().collect();
        let mut edits = Vec::new();
        let mut cursor = 0usize;
        if chars.is_empty() {
            edits.push(Edit::insert(Offset(0), "Z"));
        } else {
            edits.push(Edit {
                start: Offset(0),
                old: chars[0].to_string(),
                new: String::new(),
            });
            cursor = 1;
        }
        for (gap, old_len, new_text) in parts {
            if cursor > chars.len() {
                break;
            }
            if cursor == chars.len() {
                edits.push(Edit::insert(Offset(cursor), new_text.clone()));
                break;
            }
            let room = chars.len() - cursor;
            cursor += gap % (room + 1);
            if cursor == chars.len() {
                edits.push(Edit::insert(Offset(cursor), new_text.clone()));
                break;
            }
            let room = chars.len() - cursor;
            let take = old_len % (room + 1);
            if take == 0 {
                edits.push(Edit::insert(Offset(cursor), new_text.clone()));
                cursor += 1;
                continue;
            }
            let old: String = chars[cursor..cursor + take].iter().collect();
            edits.push(Edit {
                start: Offset(cursor),
                old,
                new: new_text.clone(),
            });
            cursor += take;
        }
        Transaction::new(edits).expect("generator builds disjoint edits")
    }

    fn random_case() -> impl Strategy<Value = (String, Transaction)> {
        arb_text().prop_flat_map(|text| {
            prop::collection::vec((0usize..6, 0usize..6, arb_snippet()), 0..5).prop_map(
                move |parts| {
                    let tx = build_transaction(&text, &parts);
                    (text.clone(), tx)
                },
            )
        })
    }

    fn empty_transaction() -> (String, Transaction) {
        (
            "hello".to_string(),
            Transaction::new(vec![]).expect("an empty list is valid"),
        )
    }

    fn same_point_inserts() -> (String, Transaction) {
        let tx = Transaction::new(vec![
            Edit::insert(Offset(1), "X"),
            Edit::insert(Offset(1), "Y"),
        ])
        .expect("two inserts at one offset do not overlap");
        ("ab".to_string(), tx)
    }

    fn adjacent_deletes_out_of_order() -> (String, Transaction) {
        let tx = Transaction::new(vec![
            Edit::delete(Offset(2), "cd"),
            Edit::delete(Offset(0), "ab"),
        ])
        .expect("touching edits do not overlap");
        ("abcd".to_string(), tx)
    }

    fn newline_and_scalar_replace() -> (String, Transaction) {
        let tx = Transaction::single(Edit {
            start: Offset(0),
            old: "é".to_string(),
            new: "ab\n".to_string(),
        });
        ("é\n你".to_string(), tx)
    }

    fn text_and_transaction() -> impl Strategy<Value = (String, Transaction)> {
        prop_oneof![
            10 => random_case(),
            1 => Just(empty_transaction()),
            1 => Just(same_point_inserts()),
            1 => Just(adjacent_deletes_out_of_order()),
            1 => Just(newline_and_scalar_replace()),
            1 => Just(delete_beside_insert()),
        ]
    }

    fn delete_beside_insert() -> (String, Transaction) {
        let tx = Transaction::new(vec![
            Edit::delete(Offset(0), "a"),
            Edit::insert(Offset(1), "b"),
        ])
        .expect("a delete and an insert at its end do not overlap");
        ("a".to_string(), tx)
    }

    fn oracle_line_col(chars: &[char], offset: usize) -> (u32, u32) {
        let mut line = 0u32;
        let mut col = 0u32;
        for (index, ch) in chars.iter().enumerate() {
            if index == offset {
                return (line, col);
            }
            if *ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn line_count_for(text: &str) -> u32 {
        1 + text.chars().filter(|ch| *ch == '\n').count() as u32
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn apply_then_apply_inverse_is_identity((text, tx) in text_and_transaction()) {
            let expected = text_after(&text, &tx);
            let mut buf = TextBuffer::from_str(&text);
            let rev = buf.revision();
            let snap = buf.snapshot();
            let first = buf.apply(&tx).unwrap();
            prop_assert_eq!(buf.text().to_string(), expected);
            prop_assert_eq!(first, Revision(rev.0 + 1));
            prop_assert_eq!(buf.revision(), first);
            prop_assert_eq!(snap.to_string(), text.clone());
            let second = buf.apply(&tx.invert()).unwrap();
            prop_assert_eq!(second, Revision(rev.0 + 2));
            prop_assert_eq!(buf.text().to_string(), text);
            prop_assert_eq!(buf.revision(), second);
        }
    }

    #[test]
    fn old_text_mismatch_is_rejected() {
        let mut buf = TextBuffer::from_str("héllo");
        let rev = buf.revision();
        assert_eq!(rev, Revision(0));
        let before = buf.snapshot();
        let tx = Transaction::single(Edit {
            start: Offset(0),
            old: "hex".to_string(),
            new: "Q".to_string(),
        });
        assert_eq!(
            buf.apply(&tx),
            Err(ApplyError::OldTextMismatch {
                edit: 0,
                expected: "hex".to_string(),
                found: "hél".to_string(),
            })
        );
        assert_eq!(buf.snapshot(), before);
        assert_eq!(buf.revision(), rev);

        let tx = Transaction::new(vec![
            Edit::delete(Offset(0), "hé"),
            Edit {
                start: Offset(2),
                old: "XYZ".to_string(),
                new: String::new(),
            },
        ])
        .expect("the two edits do not overlap");
        assert_eq!(
            buf.apply(&tx),
            Err(ApplyError::OldTextMismatch {
                edit: 1,
                expected: "XYZ".to_string(),
                found: "llo".to_string(),
            })
        );
        assert_eq!(buf.snapshot(), before);
        assert_eq!(buf.revision(), rev);
        assert_eq!(buf.text().to_string(), "héllo");
    }

    #[test]
    fn out_of_bounds_is_rejected() {
        let mut buf = TextBuffer::from_str("ab");
        let rev = buf.revision();
        let before = buf.snapshot();
        let tx = Transaction::new(vec![
            Edit::insert(Offset(2), "Z"),
            Edit::delete(Offset(3), "nope"),
        ])
        .expect("the insert and the delete do not overlap");
        assert_eq!(
            buf.apply(&tx),
            Err(ApplyError::OutOfBounds {
                edit: 1,
                end: Offset(7),
                len: Offset(2),
            })
        );
        assert_eq!(buf.snapshot(), before);
        assert_eq!(buf.revision(), rev);
        assert_eq!(buf.text().to_string(), "ab");
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn line_col_round_trips_on_every_boundary(text in arb_multiline()) {
            let buf = TextBuffer::from_str(&text);
            let chars: Vec<char> = text.chars().collect();
            // The last index is the offset after the last scalar.
            for index in 0..=chars.len() {
                let at = Offset(index);
                let (line, col) = oracle_line_col(&chars, index);
                let actual = buf.line_col(at);
                prop_assert_eq!(actual, LineCol { line, col });
                prop_assert_eq!(buf.offset(actual), at);
            }
            prop_assert_eq!(buf.line_count(), line_count_for(&text));
            prop_assert_eq!(buf.len(), Offset(chars.len()));

            let whole = CharRange::new(buf.len(), Offset(0));
            prop_assert_eq!(whole.start(), Offset(0));
            prop_assert_eq!(whole.end(), buf.len());
            prop_assert_eq!(buf.slice(whole).to_string(), text);

            let cr = TextBuffer::from_str("a\rb");
            prop_assert_eq!(cr.line_count(), 1);
            prop_assert_eq!(cr.line_col(Offset(2)), LineCol { line: 0, col: 2 });

            let empty = TextBuffer::new();
            prop_assert_eq!(empty.line_count(), 1);
            prop_assert_eq!(empty.len(), Offset(0));
            prop_assert_eq!(empty.line_col(Offset(0)), LineCol { line: 0, col: 0 });
            prop_assert_eq!(empty.offset(LineCol { line: 0, col: 0 }), Offset(0));
            prop_assert_eq!(empty.offset(LineCol { line: 4, col: 9 }), Offset(0));

            let sample = TextBuffer::from_str("ab\ncd");
            prop_assert_eq!(sample.line_col(Offset(100)), sample.line_col(sample.len()));
            prop_assert_eq!(
                sample.offset(LineCol { line: 50, col: 0 }),
                sample.offset(LineCol {
                    line: sample.line_count() - 1,
                    col: 0,
                })
            );
            prop_assert_eq!(sample.offset(LineCol { line: 0, col: 100 }), Offset(2));
            prop_assert_eq!(sample.offset(LineCol { line: 1, col: 100 }), sample.len());
        }
    }
}
