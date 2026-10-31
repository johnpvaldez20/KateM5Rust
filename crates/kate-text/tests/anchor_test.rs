// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Regression tests for anchors and the edit log.
//!
//! `track_transaction` applies the packet rule to each edit.
//! It walks the edits from last to first.

use kate_text::{Bias, Edit, Offset, Revision, TextBuffer, Transaction, transform_offset};

fn scalar_len(text: &str) -> usize {
    text.chars().count()
}

/// Independent copy of the packet rule. It does not call `transform_offset`.
fn step_offset(offset: usize, bias: Bias, edit: &Edit) -> usize {
    let start = edit.start.0;
    let old_len = scalar_len(&edit.old);
    let new_len = scalar_len(&edit.new);
    let old_end = start + old_len;
    if offset < start {
        return offset;
    }
    if offset > old_end {
        if new_len >= old_len {
            return offset + (new_len - old_len);
        }
        return offset - (old_len - new_len);
    }
    if bias == Bias::Left {
        start
    } else {
        start + new_len
    }
}

fn track_transaction(offset: usize, bias: Bias, tx: &Transaction) -> usize {
    let mut at = offset;
    for edit in tx.edits().iter().rev() {
        at = step_offset(at, bias, edit);
    }
    at
}

fn eager_transaction(offset: Offset, bias: Bias, tx: &Transaction) -> Offset {
    let mut at = offset;
    for edit in tx.edits().iter().rev() {
        at = transform_offset(at, bias, edit);
    }
    at
}

fn locked_buffer(text: &str) -> TextBuffer {
    let mut buf = TextBuffer::from_str(text);
    buf.lock_revision(Revision(0));
    buf
}

fn resolve_after(text: &str, offset: usize, bias: Bias, tx: &Transaction) -> Offset {
    let mut buf = locked_buffer(text);
    let anchor = buf.anchor(Offset(offset), bias);
    buf.apply(tx).expect("the edit applies");
    buf.resolve(anchor).expect("the anchor resolves")
}

mod anchor {
    mod tests {
        use super::super::{
            eager_transaction, locked_buffer, resolve_after, scalar_len, track_transaction,
        };
        use kate_text::{
            Anchor, Bias, Edit, Offset, Revision, TextBuffer, Transaction, transform_offset,
        };
        use proptest::prelude::*;
        use proptest::test_runner::RngSeed;

        const PROPTEST_SEED: u64 = 0x504B_5400_0000_0003;

        fn fixed_config() -> ProptestConfig {
            ProptestConfig {
                cases: 256,
                rng_seed: RngSeed::Fixed(PROPTEST_SEED),
                failure_persistence: None,
                ..ProptestConfig::default()
            }
        }

        #[test]
        fn anchor_after_edit_resolves_to_shifted_offset() {
            let text = "abcdefghijklmnopqrstuvwxyz";
            let insert_before = Transaction::single(Edit::insert(Offset(4), "xyz"));
            let insert_after = Transaction::single(Edit::insert(Offset(20), "xyz"));
            for bias in [Bias::Left, Bias::Right] {
                assert_eq!(
                    resolve_after(text, 10, bias, &insert_before),
                    Offset(13),
                    "insert before the anchor shifts it by three scalars"
                );
                assert_eq!(
                    resolve_after(text, 10, bias, &insert_after),
                    Offset(10),
                    "insert after the anchor leaves it in place"
                );
            }

            let mut buf = locked_buffer(text);
            let anchor = buf.anchor(Offset(10), Bias::Left);
            buf.apply(&insert_before).expect("the insert applies");
            assert_eq!(
                buf.transform(Offset(10), Bias::Left, Revision(0), buf.revision()),
                Ok(Offset(13))
            );
            let refreshed = buf.refresh(anchor).expect("refresh keeps the anchor");
            assert_eq!(
                refreshed,
                Anchor {
                    offset: Offset(13),
                    revision: Revision(1),
                    bias: Bias::Left,
                }
            );

            // "é" is one scalar and two bytes. The anchor moves by one scalar.
            let deleted = Transaction::single(Edit::delete(Offset(1), "é"));
            assert_eq!(resolve_after("aé你b", 3, Bias::Left, &deleted), Offset(2));
            assert_eq!(resolve_after("aé你b", 3, Bias::Right, &deleted), Offset(2));

            // Apply order is back to front. Forward order would leave the right anchor at 4.
            let both = Transaction::new(vec![
                Edit::insert(Offset(0), "Q"),
                Edit {
                    start: Offset(1),
                    old: "bc".to_string(),
                    new: "XYZ".to_string(),
                },
            ])
            .expect("the insert and the replace do not overlap");
            assert_eq!(resolve_after("abcd", 1, Bias::Right, &both), Offset(5));
            assert_eq!(resolve_after("abcd", 1, Bias::Left, &both), Offset(2));
            let mut buf = locked_buffer("abcd");
            buf.apply(&both).expect("the transaction applies");
            assert_eq!(buf.text().to_string(), "QaXYZd");
        }

        #[test]
        fn bias_decides_insert_at_anchor() {
            let insert = Transaction::single(Edit::insert(Offset(2), "XYZ"));
            assert_eq!(resolve_after("hello", 2, Bias::Left, &insert), Offset(2));
            assert_eq!(resolve_after("hello", 2, Bias::Right, &insert), Offset(5));
            let mut buf = locked_buffer("hello");
            buf.apply(&insert).expect("the insert applies");
            assert_eq!(buf.text().to_string(), "heXYZllo");

            let replace = Transaction::single(Edit {
                start: Offset(2),
                old: "cdef".to_string(),
                new: "XYZ".to_string(),
            });
            // Offset 6 is the end of the old span. Left stays at the start. Right follows `new`.
            assert_eq!(
                resolve_after("abcdefghij", 6, Bias::Left, &replace),
                Offset(2)
            );
            assert_eq!(
                resolve_after("abcdefghij", 6, Bias::Right, &replace),
                Offset(5)
            );
            assert_eq!(
                resolve_after("abcdefghij", 4, Bias::Left, &replace),
                Offset(2)
            );
            assert_eq!(
                resolve_after("abcdefghij", 4, Bias::Right, &replace),
                Offset(5)
            );
            assert_eq!(
                resolve_after("abcdefghij", 7, Bias::Left, &replace),
                Offset(6)
            );
            assert_eq!(
                resolve_after("abcdefghij", 7, Bias::Right, &replace),
                Offset(6)
            );

            let same_point = Transaction::new(vec![
                Edit::insert(Offset(1), "Y"),
                Edit::insert(Offset(1), "X"),
            ])
            .expect("two inserts at one offset do not overlap");
            assert_eq!(resolve_after("ab", 1, Bias::Left, &same_point), Offset(1));
            assert_eq!(resolve_after("ab", 1, Bias::Right, &same_point), Offset(3));

            let at_end = Edit::insert(Offset(usize::MAX), "x");
            assert_eq!(
                transform_offset(Offset(usize::MAX), Bias::Left, &at_end),
                Offset(usize::MAX)
            );
            assert_eq!(
                transform_offset(Offset(usize::MAX), Bias::Right, &at_end),
                Offset(usize::MAX)
            );
            let mut buf = locked_buffer("ab");
            let anchor = buf.anchor(Offset(usize::MAX), Bias::Right);
            buf.apply(&Transaction::single(Edit::insert(Offset(0), "x")))
                .expect("the insert applies");
            assert_eq!(
                buf.resolve(anchor).expect("huge offset resolves"),
                Offset(usize::MAX)
            );
        }

        proptest! {
            #![proptest_config(fixed_config())]
            #[test]
            fn resolve_equals_eager_tracking(case in arb_case()) {
                let mut buf = TextBuffer::from_str(&case.text);
                buf.lock_revision(Revision(0));
                let anchor = buf.anchor(Offset(case.offset), case.bias);
                let mut tracked = case.offset;
                let mut eager = Offset(case.offset);
                let mut revision = Revision(0);
                let mut first = true;
                for tx in &case.steps {
                    let before = tracked;
                    buf.apply(tx).expect("generated edit applies");
                    tracked = track_transaction(tracked, case.bias, tx);
                    eager = eager_transaction(eager, case.bias, tx);
                    if first {
                        prop_assert_ne!(Offset(tracked), Offset(case.offset));
                        first = false;
                    }
                    prop_assert_eq!(eager, Offset(tracked));
                    prop_assert_eq!(buf.resolve(anchor), Ok(Offset(tracked)));
                    prop_assert_eq!(
                        buf.refresh(anchor),
                        Ok(Anchor {
                            offset: Offset(tracked),
                            revision: buf.revision(),
                            bias: case.bias,
                        })
                    );
                    prop_assert_eq!(
                        buf.transform(Offset(before), case.bias, revision, buf.revision()),
                        Ok(Offset(tracked))
                    );
                    revision = buf.revision();
                }
            }
        }

        #[derive(Clone, Debug)]
        struct Case {
            text: String,
            offset: usize,
            bias: Bias,
            steps: Vec<Transaction>,
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

        fn arb_snippet() -> impl Strategy<Value = String> {
            prop::collection::vec(arb_scalar(), 0..6).prop_map(|chars| chars.into_iter().collect())
        }

        fn arb_text() -> impl Strategy<Value = String> {
            prop::collection::vec(arb_scalar(), 1..20).prop_map(|chars| chars.into_iter().collect())
        }

        fn arb_bias() -> impl Strategy<Value = Bias> {
            prop_oneof![Just(Bias::Left), Just(Bias::Right)]
        }

        fn arb_batches() -> impl Strategy<Value = Vec<Vec<(usize, usize, String)>>> {
            let batch = prop::collection::vec((0usize..8, 0usize..8, arb_snippet()), 0..4);
            prop::collection::vec(batch, 0..4)
        }

        fn arb_case() -> impl Strategy<Value = Case> {
            arb_text().prop_flat_map(|text| {
                let len = scalar_len(&text);
                (Just(text), 1..=len, arb_bias(), arb_batches()).prop_map(
                    |(text, offset, bias, batches)| assemble_case(text, offset, bias, batches),
                )
            })
        }

        fn assemble_case(
            text: String,
            offset: usize,
            bias: Bias,
            batches: Vec<Vec<(usize, usize, String)>>,
        ) -> Case {
            let mut current = text.clone();
            let mut steps = Vec::new();
            let first = Transaction::single(Edit::insert(Offset(0), "Q"));
            current = text_after(&current, &first);
            steps.push(first);
            for batch in &batches {
                let tx = build_tx(&current, batch);
                current = text_after(&current, &tx);
                steps.push(tx);
            }
            Case {
                text,
                offset,
                bias,
                steps,
            }
        }

        fn text_after(text: &str, tx: &Transaction) -> String {
            let mut buf = TextBuffer::from_str(text);
            buf.apply(tx)
                .expect("the generator builds a valid transaction");
            buf.text().to_string()
        }

        fn build_tx(text: &str, parts: &[(usize, usize, String)]) -> Transaction {
            let chars: Vec<char> = text.chars().collect();
            let mut edits = Vec::new();
            let mut cursor = 0usize;
            for (gap, old_len, new_text) in parts {
                if cursor > chars.len() {
                    break;
                }
                let room = chars.len() - cursor;
                let skip = if room == 0 { 0 } else { gap % (room + 1) };
                cursor += skip;
                let room = chars.len() - cursor;
                let take = if room == 0 { 0 } else { old_len % (room + 1) };
                let old: String = chars[cursor..cursor + take].iter().collect();
                edits.push(Edit {
                    start: Offset(cursor),
                    old,
                    new: new_text.clone(),
                });
                cursor += take;
                if cursor == chars.len() && take == 0 {
                    break;
                }
            }
            Transaction::new(edits).expect("the generator builds disjoint edits")
        }
    }
}

mod log {
    mod tests {
        use kate_text::{
            Bias, Edit, EditLog, Offset, Revision, StaleAnchor, TextBuffer, Transaction,
        };

        fn insert_ab() -> Transaction {
            Transaction::single(Edit::insert(Offset(0), "ab"))
        }

        #[test]
        fn unlocked_history_is_pruned_and_stale_is_an_error() {
            assert_two_edits_without_a_lock_are_stale();
            assert_lock_keeps_revision_zero();
            assert_refcount_and_unlock_prune();
            assert_lock_of_the_current_revision_keeps_existing_history();
            assert_rejected_apply_does_not_advance_the_log();
            assert_direct_log_rejects_a_gap_and_a_bad_revision();
            assert_bad_transform_input_returns_an_error();
        }

        fn assert_two_edits_without_a_lock_are_stale() {
            let mut bare = TextBuffer::from_str("hello");
            let origin = bare.anchor(Offset(1), Bias::Left);
            bare.apply(&insert_ab()).expect("first edit applies");
            assert_eq!(bare.resolve(origin), Ok(Offset(3)));
            let at_one = bare.anchor(Offset(3), Bias::Left);
            bare.apply(&Transaction::single(Edit::insert(Offset(0), "cd")))
                .expect("second edit applies");
            assert_eq!(
                bare.resolve(origin),
                Err(StaleAnchor {
                    requested: Revision(0),
                    oldest_available: Revision(1),
                })
            );
            assert_eq!(
                bare.refresh(origin),
                Err(StaleAnchor {
                    requested: Revision(0),
                    oldest_available: Revision(1),
                })
            );
            assert_eq!(bare.resolve(at_one), Ok(Offset(5)));
        }

        fn assert_lock_keeps_revision_zero() {
            let mut locked = TextBuffer::from_str("hello");
            locked.lock_revision(Revision(0));
            let anchor = locked.anchor(Offset(1), Bias::Left);
            locked.apply(&insert_ab()).expect("first edit applies");
            locked
                .apply(&Transaction::single(Edit::insert(Offset(0), "cd")))
                .expect("second edit applies");
            assert_eq!(locked.resolve(anchor), Ok(Offset(5)));
            assert_eq!(locked.revision(), Revision(2));
            let refreshed = locked.refresh(anchor).expect("locked anchor refreshes");
            assert_eq!(refreshed.offset, Offset(5));
            assert_eq!(refreshed.revision, Revision(2));
            assert_eq!(refreshed.bias, Bias::Left);
        }

        fn assert_refcount_and_unlock_prune() {
            let mut buf = TextBuffer::from_str("hello");
            buf.lock_revision(Revision(0));
            buf.lock_revision(Revision(0));
            buf.unlock_revision(Revision(0));
            let anchor = buf.anchor(Offset(1), Bias::Left);
            buf.apply(&insert_ab()).expect("first edit applies");
            buf.apply(&Transaction::single(Edit::insert(Offset(0), "cd")))
                .expect("second edit applies");
            assert_eq!(buf.resolve(anchor), Ok(Offset(5)));
            buf.unlock_revision(Revision(0));
            assert_eq!(
                buf.resolve(anchor),
                Err(StaleAnchor {
                    requested: Revision(0),
                    oldest_available: Revision(1),
                })
            );
        }

        fn assert_lock_of_the_current_revision_keeps_existing_history() {
            let mut buf = TextBuffer::from_str("hello");
            let origin = buf.anchor(Offset(1), Bias::Left);
            buf.apply(&insert_ab()).expect("first edit applies");
            assert_eq!(buf.resolve(origin), Ok(Offset(3)));
            let current = buf.revision();
            buf.lock_revision(current);
            assert_eq!(buf.resolve(origin), Ok(Offset(3)));
            buf.apply(&Transaction::single(Edit::insert(Offset(0), "cd")))
                .expect("second edit applies");
            assert_eq!(
                buf.resolve(origin),
                Err(StaleAnchor {
                    requested: Revision(0),
                    oldest_available: Revision(1),
                })
            );
        }

        fn assert_rejected_apply_does_not_advance_the_log() {
            let mut buf = TextBuffer::from_str("ab");
            buf.lock_revision(Revision(0));
            let anchor = buf.anchor(Offset(1), Bias::Right);
            let bad = Transaction::single(Edit::delete(Offset(0), "zz"));
            assert!(buf.apply(&bad).is_err());
            assert_eq!(buf.revision(), Revision(0));
            assert_eq!(buf.resolve(anchor), Ok(Offset(1)));
            buf.apply(&Transaction::single(Edit::insert(Offset(0), "q")))
                .expect("the valid edit applies");
            assert_eq!(buf.resolve(anchor), Ok(Offset(2)));
        }

        fn assert_direct_log_rejects_a_gap_and_a_bad_revision() {
            let tx = Transaction::single(Edit::insert(Offset(0), "a"));
            let mut log = EditLog::new(Revision(0));
            log.push(tx.clone(), Revision(4));
            log.push(tx.clone(), Revision(0));
            assert_eq!(log.oldest_available(), Revision(0));
            assert!(
                log.entries_since(Revision(0))
                    .expect("revision 0 is live")
                    .next()
                    .is_none()
            );
            log.lock(Revision(9));
            log.unlock(Revision(3));
            log.unlock(Revision(0));
            log.push(tx.clone(), Revision(1));
            log.push(tx, Revision(2));
            assert_eq!(log.oldest_available(), Revision(1));
            assert!(log.entries_since(Revision(0)).is_err());
            assert_eq!(
                log.entries_since(Revision(1))
                    .expect("revision 1 is live")
                    .count(),
                1
            );

            let mut kept = EditLog::new(Revision(0));
            kept.lock(Revision(0));
            let tx = Transaction::single(Edit::insert(Offset(0), "a"));
            kept.push(tx.clone(), Revision(1));
            kept.push(tx, Revision(2));
            assert_eq!(kept.oldest_available(), Revision(0));
            assert_eq!(
                kept.entries_since(Revision(0))
                    .expect("lock keeps both")
                    .count(),
                2
            );
        }

        fn assert_bad_transform_input_returns_an_error() {
            let mut buf = TextBuffer::from_str("ab");
            buf.lock_revision(Revision(9));
            buf.unlock_revision(Revision(4));
            buf.unlock_revision(Revision(0));
            assert!(
                buf.transform(Offset(1), Bias::Left, Revision(3), Revision(1))
                    .is_err()
            );
            assert!(
                buf.transform(Offset(0), Bias::Left, Revision(0), Revision(5))
                    .is_err()
            );
            assert_eq!(
                buf.transform(Offset(1), Bias::Left, Revision(0), Revision(0)),
                Ok(Offset(1))
            );
            assert_eq!(
                buf.transform(Offset(usize::MAX), Bias::Right, Revision(4), Revision(4)),
                Ok(Offset(usize::MAX))
            );
        }
    }
}
