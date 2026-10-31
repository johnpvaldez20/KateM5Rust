// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Regression tests for undo, redo, coalescing, and the saved depth.
//!
//! The names match the PKT-0004 verification plan.

use kate_text::{
    CharRange, Edit, EditKind, History, HistoryEntry, Offset, TextBuffer, Transaction,
};

fn caret(at: usize) -> Vec<CharRange> {
    vec![CharRange::new(Offset(at), Offset(at))]
}

fn record(
    history: &mut History,
    buf: &mut TextBuffer,
    tx: Transaction,
    kind: EditKind,
    ranges_before: Vec<CharRange>,
    ranges_after: Vec<CharRange>,
) {
    let revision_before = buf.revision();
    let inverse = tx.invert();
    let revision_after = buf.apply(&tx).expect("the edit applies");
    history.record(HistoryEntry {
        forward: tx,
        inverse,
        kind,
        revision_before,
        revision_after,
        ranges_before,
        ranges_after,
    });
}

fn type_char(history: &mut History, buf: &mut TextBuffer, ch: char) {
    let at = buf.len().0;
    let tx = Transaction::single(Edit::insert(Offset(at), ch.to_string()));
    record(history, buf, tx, EditKind::Typing, caret(at), caret(at + 1));
}

fn backspace(history: &mut History, buf: &mut TextBuffer) {
    let chars: Vec<char> = buf.text().chars().collect();
    let ch = chars[chars.len() - 1];
    let at = chars.len() - 1;
    let tx = Transaction::single(Edit::delete(Offset(at), ch.to_string()));
    record(
        history,
        buf,
        tx,
        EditKind::Deletion,
        caret(at + 1),
        caret(at),
    );
}

fn paste(
    history: &mut History,
    buf: &mut TextBuffer,
    tx: Transaction,
    before: usize,
    after: usize,
) {
    record(
        history,
        buf,
        tx,
        EditKind::Paste,
        caret(before),
        caret(after),
    );
}

fn text_of(buf: &TextBuffer) -> String {
    buf.text().to_string()
}

mod history {
    mod tests {
        use super::super::{backspace, caret, paste, record, text_of, type_char};
        use kate_text::{
            CharRange, Edit, EditKind, History, Offset, Revision, TextBuffer, Transaction,
        };
        use proptest::prelude::*;
        use proptest::test_runner::RngSeed;

        const PROPTEST_SEED: u64 = 0x504B_5400_0000_0004;

        fn fixed_config() -> ProptestConfig {
            ProptestConfig {
                cases: 256,
                rng_seed: RngSeed::Fixed(PROPTEST_SEED),
                failure_persistence: None,
                ..ProptestConfig::default()
            }
        }

        #[test]
        fn undo_redo_round_trip_restores_text_and_ranges() {
            assert_empty_stacks_and_a_rejected_undo_do_not_panic();
            assert_known_sequences_round_trip();
            let config = fixed_config();
            proptest!(config, |(case in arb_case())| {
                check_case(case)?;
            });
        }

        #[test]
        fn coalesces_adjacent_single_char_inserts() {
            let mut buf = TextBuffer::new();
            let mut history = History::new();
            type_char(&mut history, &mut buf, 'a');
            type_char(&mut history, &mut buf, 'b');
            type_char(&mut history, &mut buf, 'c');
            assert_eq!(text_of(&buf), "abc");

            let rev = buf.revision();
            let ranges = history
                .undo(&mut buf)
                .expect("one undo removes a, b, and c");
            assert_eq!(ranges, caret(0));
            assert_eq!(text_of(&buf), "");
            assert!(!history.can_undo());
            assert_eq!(buf.revision(), Revision(rev.0 + 1));

            let restored = history
                .redo(&mut buf)
                .expect("redo restores the three chars");
            assert_eq!(restored, caret(3));
            assert_eq!(text_of(&buf), "abc");
            assert!(!history.can_redo());

            // A later char that does not touch the previous end stays its own entry.
            let mut buf = TextBuffer::from_str("zzzz");
            let mut history = History::new();
            let at_start = Transaction::single(Edit::insert(Offset(0), "A"));
            record(
                &mut history,
                &mut buf,
                at_start,
                EditKind::Typing,
                caret(0),
                caret(1),
            );
            let at_end = Transaction::single(Edit::insert(Offset(5), "B"));
            record(
                &mut history,
                &mut buf,
                at_end,
                EditKind::Typing,
                caret(5),
                caret(6),
            );
            assert_eq!(text_of(&buf), "AzzzzB");
            history.undo(&mut buf).expect("undo only B");
            assert_eq!(text_of(&buf), "Azzzz");
            assert!(history.can_undo());
            history.undo(&mut buf).expect("undo A");
            assert_eq!(text_of(&buf), "zzzz");
            assert!(!history.can_undo());

            // end_group seals the entry. The next adjacent char starts a new entry.
            let mut buf = TextBuffer::new();
            let mut history = History::new();
            history.begin_group();
            type_char(&mut history, &mut buf, 'q');
            type_char(&mut history, &mut buf, 'r');
            history.end_group();
            type_char(&mut history, &mut buf, 's');
            assert_eq!(text_of(&buf), "qrs");
            history.undo(&mut buf).expect("undo s");
            assert_eq!(text_of(&buf), "qr");
            assert!(history.can_undo());
            history.undo(&mut buf).expect("undo the grouped qr");
            assert_eq!(text_of(&buf), "");
            assert!(!history.can_undo());
        }

        #[test]
        fn saved_state_tracks_undo_depth() {
            let mut buf = TextBuffer::new();
            let mut history = History::new();
            assert!(!history.is_modified());

            type_char(&mut history, &mut buf, 'a');
            assert!(history.is_modified());
            assert_eq!(text_of(&buf), "a");

            history.mark_saved();
            assert!(!history.is_modified());

            type_char(&mut history, &mut buf, 'b');
            assert!(history.is_modified());
            assert_eq!(text_of(&buf), "ab");

            history
                .undo(&mut buf)
                .expect("undo the edit after the save");
            assert_eq!(text_of(&buf), "a");
            assert!(!history.is_modified());

            history.undo(&mut buf).expect("undo past the saved depth");
            assert_eq!(text_of(&buf), "");
            assert!(history.is_modified());

            type_char(&mut history, &mut buf, 'c');
            assert_eq!(text_of(&buf), "c");
            assert!(history.is_modified());
            assert!(!history.can_redo());

            history.undo(&mut buf).expect("undo the branch");
            assert_eq!(text_of(&buf), "");
            assert!(history.is_modified());
            assert!(!history.can_undo());
        }

        #[test]
        fn new_edit_clears_redo() {
            let mut buf = TextBuffer::from_str("q");
            let mut history = History::new();
            paste(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(1), "X")),
                1,
                2,
            );
            paste(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(2), "Y")),
                2,
                3,
            );
            assert_eq!(text_of(&buf), "qXY");
            history.undo(&mut buf).expect("undo Y");
            assert_eq!(text_of(&buf), "qX");
            assert!(history.can_redo());
            history.undo(&mut buf).expect("undo X");
            assert_eq!(text_of(&buf), "q");
            assert!(history.can_redo());

            paste(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(1), "Z")),
                1,
                2,
            );
            assert_eq!(text_of(&buf), "qZ");
            assert!(!history.can_redo());
            assert_eq!(history.redo(&mut buf), None);
            assert_eq!(text_of(&buf), "qZ");
        }

        fn assert_empty_stacks_and_a_rejected_undo_do_not_panic() {
            let mut history = History::new();
            let mut buf = TextBuffer::new();
            assert_eq!(history.undo(&mut buf), None);
            assert_eq!(history.redo(&mut buf), None);
            assert!(!history.can_undo());
            assert!(!history.can_redo());
            assert!(!history.is_modified());
            history.begin_group();
            history.end_group();
            history.clear();
            assert!(!history.is_modified());

            let mut buf = TextBuffer::from_str("hi");
            let mut history = History::new();
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(0), "Z")),
                EditKind::Paste,
                caret(0),
                caret(1),
            );
            assert_eq!(text_of(&buf), "Zhi");
            buf.apply(&Transaction::single(Edit::delete(Offset(0), "Z")))
                .expect("the direct delete applies");
            let rev = buf.revision();
            let before = text_of(&buf);
            assert_eq!(history.undo(&mut buf), None);
            assert_eq!(text_of(&buf), before);
            assert_eq!(buf.revision(), rev);
            assert!(history.can_undo());
            assert!(!history.can_redo());
        }

        fn assert_known_sequences_round_trip() {
            let mut buf = TextBuffer::from_str("hello");
            let mut history = History::new();
            let first_before = caret(0);
            let first = Transaction::new(vec![
                Edit::insert(Offset(0), "Q"),
                Edit::delete(Offset(2), "ll"),
            ])
            .expect("the insert and the delete do not overlap");
            record(
                &mut history,
                &mut buf,
                first,
                EditKind::Replace,
                first_before.clone(),
                caret(4),
            );
            assert_eq!(text_of(&buf), "Qheo");
            let second_before = caret(4);
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(4), "!")),
                EditKind::Programmatic,
                second_before.clone(),
                caret(5),
            );
            assert_eq!(text_of(&buf), "Qheo!");
            let final_text = text_of(&buf);

            let rev = buf.revision();
            let got = history.undo(&mut buf).expect("undo the bang");
            assert_eq!(got, second_before);
            assert_eq!(text_of(&buf), "Qheo");
            assert_eq!(buf.revision(), Revision(rev.0 + 1));
            let got = history.undo(&mut buf).expect("undo the replace");
            assert_eq!(got, first_before);
            assert_eq!(text_of(&buf), "hello");
            assert!(!history.can_undo());

            let got = history.redo(&mut buf).expect("redo the replace");
            assert_eq!(got, caret(4));
            assert_eq!(text_of(&buf), "Qheo");
            let got = history.redo(&mut buf).expect("redo the bang");
            assert_eq!(got, caret(5));
            assert_eq!(text_of(&buf), final_text);

            let mut buf = TextBuffer::from_str("abcd");
            let mut history = History::new();
            backspace(&mut history, &mut buf);
            backspace(&mut history, &mut buf);
            backspace(&mut history, &mut buf);
            assert_eq!(text_of(&buf), "a");
            history
                .undo(&mut buf)
                .expect("one undo restores the three deletions");
            assert_eq!(text_of(&buf), "abcd");
            assert!(!history.can_undo());
            history.redo(&mut buf).expect("redo the deletions");
            assert_eq!(text_of(&buf), "a");

            let mut buf = TextBuffer::from_str("hello");
            let mut history = History::new();
            history.begin_group();
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::insert(Offset(0), "X")),
                EditKind::Paste,
                caret(0),
                caret(1),
            );
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::delete(Offset(1), "h")),
                EditKind::Deletion,
                caret(1),
                caret(1),
            );
            history.end_group();
            assert_eq!(text_of(&buf), "Xello");
            let ranges = history.undo(&mut buf).expect("one undo restores the group");
            assert_eq!(ranges, caret(0));
            assert_eq!(text_of(&buf), "hello");
            assert!(!history.can_undo());
            let ranges = history.redo(&mut buf).expect("redo the group");
            assert_eq!(ranges, caret(1));
            assert_eq!(text_of(&buf), "Xello");

            // A forward delete does not meet the reverse edge, so it stays separate.
            let mut buf = TextBuffer::from_str("abc");
            let mut history = History::new();
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::delete(Offset(0), "a")),
                EditKind::Deletion,
                caret(0),
                caret(0),
            );
            record(
                &mut history,
                &mut buf,
                Transaction::single(Edit::delete(Offset(0), "b")),
                EditKind::Deletion,
                caret(0),
                caret(0),
            );
            assert_eq!(text_of(&buf), "c");
            history.undo(&mut buf).expect("undo only b");
            assert_eq!(text_of(&buf), "bc");
            assert!(history.can_undo());
        }

        #[derive(Clone, Debug)]
        enum Case {
            Separate { start: String, steps: Vec<Step> },
            Typing { start: String, chars: String },
            Backspace { start: String, count: usize },
            Group { start: String, steps: Vec<Step> },
        }

        #[derive(Clone, Debug)]
        struct Step {
            tx: Transaction,
            kind: EditKind,
            ranges_before: Vec<CharRange>,
            ranges_after: Vec<CharRange>,
        }

        fn check_case(case: Case) -> Result<(), TestCaseError> {
            match case {
                Case::Separate { start, steps } => check_separate(&start, &steps),
                Case::Typing { start, chars } => check_typing(&start, &chars),
                Case::Backspace { start, count } => check_backspace(&start, count),
                Case::Group { start, steps } => check_group(&start, &steps),
            }
        }

        fn check_separate(start: &str, steps: &[Step]) -> Result<(), TestCaseError> {
            let mut buf = TextBuffer::from_str(start);
            let mut history = History::new();
            for step in steps {
                record(
                    &mut history,
                    &mut buf,
                    step.tx.clone(),
                    step.kind,
                    step.ranges_before.clone(),
                    step.ranges_after.clone(),
                );
            }
            let final_text = text_of(&buf);
            prop_assert_ne!(&final_text, start);
            let undone = undo_all(&mut history, &mut buf);
            prop_assert_eq!(undone.len(), steps.len());
            prop_assert_eq!(text_of(&buf), start);
            for (got, step) in undone.iter().rev().zip(steps.iter()) {
                prop_assert_eq!(got, &step.ranges_before);
            }
            let redone = redo_all(&mut history, &mut buf);
            prop_assert_eq!(redone.len(), steps.len());
            prop_assert_eq!(text_of(&buf), final_text);
            for (got, step) in redone.iter().zip(steps.iter()) {
                prop_assert_eq!(got, &step.ranges_after);
            }
            Ok(())
        }

        fn check_typing(start: &str, chars: &str) -> Result<(), TestCaseError> {
            let mut buf = TextBuffer::from_str(start);
            let mut history = History::new();
            let first = caret(buf.len().0);
            for ch in chars.chars() {
                type_char(&mut history, &mut buf, ch);
            }
            let end = buf.len().0;
            let final_text = text_of(&buf);
            prop_assert_ne!(&final_text, start);
            let rev = buf.revision();
            let ranges = history.undo(&mut buf).expect("one undo removes the run");
            prop_assert_eq!(ranges, first);
            prop_assert_eq!(text_of(&buf), start);
            prop_assert!(!history.can_undo());
            prop_assert_eq!(buf.revision(), Revision(rev.0 + 1));
            let ranges = history.redo(&mut buf).expect("redo restores the run");
            prop_assert_eq!(ranges, caret(end));
            prop_assert_eq!(text_of(&buf), final_text);
            Ok(())
        }

        fn check_backspace(start: &str, count: usize) -> Result<(), TestCaseError> {
            let mut buf = TextBuffer::from_str(start);
            let mut history = History::new();
            for _ in 0..count {
                backspace(&mut history, &mut buf);
            }
            let final_text = text_of(&buf);
            prop_assert_ne!(&final_text, start);
            let ranges = history
                .undo(&mut buf)
                .expect("one undo restores every backspace");
            prop_assert_eq!(ranges, caret(start.chars().count()));
            prop_assert_eq!(text_of(&buf), start);
            prop_assert!(!history.can_undo());
            history.redo(&mut buf).expect("redo the backspaces");
            prop_assert_eq!(text_of(&buf), final_text);
            Ok(())
        }

        fn check_group(start: &str, steps: &[Step]) -> Result<(), TestCaseError> {
            let mut buf = TextBuffer::from_str(start);
            let mut history = History::new();
            history.begin_group();
            for step in steps {
                record(
                    &mut history,
                    &mut buf,
                    step.tx.clone(),
                    step.kind,
                    step.ranges_before.clone(),
                    step.ranges_after.clone(),
                );
            }
            history.end_group();
            let final_text = text_of(&buf);
            prop_assert_ne!(&final_text, start);
            let ranges = history.undo(&mut buf).expect("one undo restores the group");
            prop_assert_eq!(&ranges, &steps[0].ranges_before);
            prop_assert_eq!(text_of(&buf), start);
            prop_assert!(!history.can_undo());
            let ranges = history.redo(&mut buf).expect("redo the group");
            prop_assert_eq!(&ranges, &steps[steps.len() - 1].ranges_after);
            prop_assert_eq!(text_of(&buf), final_text);
            Ok(())
        }

        fn undo_all(history: &mut History, buf: &mut TextBuffer) -> Vec<Vec<CharRange>> {
            let mut got = Vec::new();
            for _ in 0..64 {
                if !history.can_undo() {
                    break;
                }
                let rev = buf.revision();
                let ranges = history.undo(buf).expect("undo applies");
                assert_eq!(buf.revision(), Revision(rev.0 + 1));
                got.push(ranges);
            }
            assert!(!history.can_undo());
            got
        }

        fn redo_all(history: &mut History, buf: &mut TextBuffer) -> Vec<Vec<CharRange>> {
            let mut got = Vec::new();
            for _ in 0..64 {
                if !history.can_redo() {
                    break;
                }
                let rev = buf.revision();
                let ranges = history.redo(buf).expect("redo applies");
                assert_eq!(buf.revision(), Revision(rev.0 + 1));
                got.push(ranges);
            }
            assert!(!history.can_redo());
            got
        }

        fn arb_scalar() -> impl Strategy<Value = char> {
            prop_oneof![
                6 => prop::char::range('a', 'z'),
                1 => Just('\n'),
                1 => Just('é'),
                1 => Just('你'),
                1 => Just('\u{1F600}'),
            ]
        }

        fn arb_text() -> impl Strategy<Value = String> {
            prop::collection::vec(arb_scalar(), 0..24).prop_map(|chars| chars.into_iter().collect())
        }

        fn arb_snippet() -> impl Strategy<Value = String> {
            prop::collection::vec(arb_scalar(), 0..6).prop_map(|chars| chars.into_iter().collect())
        }

        fn arb_case() -> impl Strategy<Value = Case> {
            prop_oneof![
                2 => arb_separate(),
                2 => arb_typing(),
                1 => arb_backspace(),
                2 => arb_group(),
            ]
        }

        fn arb_separate() -> impl Strategy<Value = Case> {
            (
                arb_text(),
                prop::collection::vec((0usize..8, 0usize..6, arb_snippet(), 0usize..4), 1..5),
            )
                .prop_map(|(start, parts)| {
                    let steps = build_steps(&start, &parts, false);
                    Case::Separate { start, steps }
                })
        }

        fn arb_typing() -> impl Strategy<Value = Case> {
            (
                arb_text(),
                prop::collection::vec(arb_scalar(), 2..7)
                    .prop_map(|chars| chars.into_iter().collect()),
            )
                .prop_map(|(start, chars)| Case::Typing { start, chars })
        }

        fn arb_backspace() -> impl Strategy<Value = Case> {
            prop::collection::vec(arb_scalar(), 2..12).prop_flat_map(|chars| {
                let len = chars.len();
                let start: String = chars.into_iter().collect();
                (Just(start), 2..=len).prop_map(|(start, count)| Case::Backspace { start, count })
            })
        }

        fn arb_group() -> impl Strategy<Value = Case> {
            (
                arb_text(),
                prop::collection::vec((0usize..8, 0usize..6, arb_snippet(), 0usize..6), 2..5),
            )
                .prop_map(|(start, parts)| {
                    let steps = build_steps(&start, &parts, true);
                    Case::Group { start, steps }
                })
        }

        fn build_steps(
            start: &str,
            parts: &[(usize, usize, String, usize)],
            grouped: bool,
        ) -> Vec<Step> {
            let mut text = start.to_string();
            let mut steps = Vec::new();
            for (index, (gap, old_len, new_text, kind_pick)) in parts.iter().enumerate() {
                let before = text.chars().count();
                let tx = one_edit(&text, *gap, *old_len, new_text);
                text = apply_shadow(&text, &tx);
                let after = text.chars().count();
                let kind = if grouped {
                    kind_from(*kind_pick)
                } else {
                    separate_kind(*kind_pick)
                };
                steps.push(Step {
                    tx,
                    kind,
                    ranges_before: vec![CharRange::new(Offset(index), Offset(before))],
                    ranges_after: vec![CharRange::new(Offset(1_000 + index), Offset(after))],
                });
            }
            if text == start {
                let at = text.chars().count();
                let tx = Transaction::single(Edit::insert(Offset(at), "Q"));
                text.push('Q');
                steps.push(Step {
                    tx,
                    kind: EditKind::Paste,
                    ranges_before: caret(at),
                    ranges_after: caret(at + 1),
                });
            }
            let _ = text;
            steps
        }

        fn separate_kind(pick: usize) -> EditKind {
            match pick % 4 {
                0 => EditKind::Paste,
                1 => EditKind::Replace,
                2 => EditKind::Indent,
                _ => EditKind::Programmatic,
            }
        }

        fn kind_from(pick: usize) -> EditKind {
            match pick % 6 {
                0 => EditKind::Typing,
                1 => EditKind::Deletion,
                2 => EditKind::Paste,
                3 => EditKind::Replace,
                4 => EditKind::Indent,
                _ => EditKind::Programmatic,
            }
        }

        fn one_edit(text: &str, gap: usize, old_len: usize, new_text: &str) -> Transaction {
            let chars: Vec<char> = text.chars().collect();
            if chars.is_empty() {
                let new_text = if new_text.is_empty() {
                    "Q".to_string()
                } else {
                    new_text.to_string()
                };
                return Transaction::single(Edit::insert(Offset(0), new_text));
            }
            let start = gap % (chars.len() + 1);
            let room = chars.len() - start;
            let take = if room == 0 { 0 } else { old_len % (room + 1) };
            let old: String = chars[start..start + take].iter().collect();
            let new_text = if take == 0 && new_text.is_empty() {
                "Q".to_string()
            } else {
                new_text.to_string()
            };
            Transaction::single(Edit {
                start: Offset(start),
                old,
                new: new_text,
            })
        }

        fn apply_shadow(text: &str, tx: &Transaction) -> String {
            let mut buf = TextBuffer::from_str(text);
            buf.apply(tx).expect("the generated edit applies");
            text_of(&buf)
        }
    }
}
