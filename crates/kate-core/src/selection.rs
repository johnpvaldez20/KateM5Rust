// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! A sorted set of selections.
//!
//! One cursor is a set of size one.
//! The set stays sorted by the minimum offset.
//! Ranges do not overlap.
//! The primary index always points at one range.

use kate_text::{CharRange, Offset, Transaction};

/// One cursor in the buffer.
///
/// `anchor == head` is a caret. `head` is the end that moves.
/// `goal_col` is the tab column kept by vertical movement.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Selection {
    pub anchor: Offset,
    pub head: Offset,
    pub goal_col: Option<u32>,
}

impl Selection {
    /// Build a caret at `at`. The caret has no goal column.
    pub fn caret(at: Offset) -> Self {
        Self {
            anchor: at,
            head: at,
            goal_col: None,
        }
    }

    /// Return the half-open range from the minimum end to the maximum end.
    pub fn range(&self) -> CharRange {
        CharRange::new(self.anchor, self.head)
    }

    /// Return true when `anchor` and `head` are the same offset.
    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
    }

    /// Return the smaller of `anchor` and `head`.
    pub fn min(&self) -> Offset {
        self.anchor.min(self.head)
    }

    /// Return the larger of `anchor` and `head`.
    pub fn max(&self) -> Offset {
        self.anchor.max(self.head)
    }
}

/// A sorted, non-overlapping set of selections.
///
/// Every public method leaves the set sorted by [`Selection::min`].
/// No two ranges overlap. `primary` is always inside the set.
/// An empty input becomes one caret at offset 0.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Selections {
    ranges: Vec<Selection>,
    primary: usize,
}

impl Selections {
    /// Build a set that holds only `s`.
    pub fn single(s: Selection) -> Self {
        Self::new(vec![s], 0)
    }

    /// Sort `ranges`, merge overlaps, and keep the primary range.
    ///
    /// An empty list becomes one caret at offset 0.
    /// A `primary` past the end uses the last input range.
    pub fn new(ranges: Vec<Selection>, primary: usize) -> Self {
        // Stub. The real normalize lands in the next commit.
        if ranges.is_empty() {
            return Self {
                ranges: vec![Selection::caret(Offset(0))],
                primary: 0,
            };
        }
        let primary = primary.min(ranges.len() - 1);
        Self { ranges, primary }
    }

    /// Return the primary selection.
    ///
    /// A broken set returns a caret at offset 0. It does not panic.
    pub fn primary(&self) -> Selection {
        self.ranges
            .get(self.primary)
            .copied()
            .unwrap_or_else(|| Selection::caret(Offset(0)))
    }

    /// Return each selection in order.
    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.ranges.iter()
    }

    /// Return the number of selections.
    ///
    /// The set always holds one selection, so `is_empty` does not apply.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Add `s` and make it the primary selection. Then normalize.
    pub fn push(&mut self, s: Selection) {
        self.ranges.push(s);
        self.primary = self.ranges.len() - 1;
    }

    /// Keep the primary selection and drop the others.
    pub fn remove_secondary(&mut self) {
        let keep = self.primary();
        *self = Self::single(keep);
    }

    /// Apply `f` to each selection. Then normalize.
    ///
    /// The primary index follows the same slot through the merge.
    pub fn map(&mut self, mut f: impl FnMut(Selection) -> Selection) {
        self.ranges = self.ranges.iter().copied().map(&mut f).collect();
    }

    /// Map every end through `tx`. Then normalize.
    ///
    /// `anchor` uses left bias. `head` uses right bias.
    /// `goal_col` stays as it is.
    pub fn transformed(&self, tx: &Transaction) -> Selections {
        let _ = tx;
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{Selection, Selections};
    use kate_text::{Edit, Offset, Transaction};
    use proptest::prelude::*;
    use proptest::test_runner::{RngSeed, TestCaseError};

    /// Fixed seed so a local run and a CI run use the same cases.
    const PROPTEST_SEED: u64 = 0x504B_5400_0000_000B;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn arb_selection() -> impl Strategy<Value = Selection> {
        (0usize..30, 0usize..30, prop::option::of(0u32..20)).prop_map(|(anchor, head, goal_col)| {
            Selection {
                anchor: Offset(anchor),
                head: Offset(head),
                goal_col,
            }
        })
    }

    fn arb_input() -> impl Strategy<Value = (Vec<Selection>, usize)> {
        prop::collection::vec(arb_selection(), 0..8).prop_flat_map(|ranges| {
            let upper = ranges.len() + 3;
            (Just(ranges), 0..upper)
        })
    }

    fn known_merge() -> (Vec<Selection>, usize) {
        (
            vec![
                Selection {
                    anchor: Offset(0),
                    head: Offset(4),
                    goal_col: None,
                },
                Selection {
                    anchor: Offset(10),
                    head: Offset(12),
                    goal_col: None,
                },
                Selection {
                    anchor: Offset(2),
                    head: Offset(3),
                    goal_col: Some(4),
                },
            ],
            2,
        )
    }

    fn known_backward() -> (Vec<Selection>, usize) {
        (
            vec![
                Selection {
                    anchor: Offset(0),
                    head: Offset(6),
                    goal_col: Some(1),
                },
                Selection {
                    anchor: Offset(8),
                    head: Offset(2),
                    goal_col: Some(9),
                },
            ],
            1,
        )
    }

    fn known_unsorted() -> (Vec<Selection>, usize) {
        (
            vec![Selection::caret(Offset(5)), Selection::caret(Offset(1))],
            0,
        )
    }

    fn arb_case() -> impl Strategy<Value = (Vec<Selection>, usize)> {
        prop_oneof![
            1 => Just(known_merge()),
            1 => Just(known_backward()),
            1 => Just(known_unsorted()),
            1 => Just((Vec::new(), 9usize)),
            8 => arb_input(),
        ]
    }

    fn ranges_overlap(left: Selection, right: Selection) -> bool {
        let left_min = left.min().0;
        let left_max = left.max().0;
        let right_min = right.min().0;
        let right_max = right.max().0;
        if left_min == left_max && right_min == right_max {
            return left_min == right_min;
        }
        if left_min == left_max {
            return right_min <= left_min && left_min < right_max;
        }
        if right_min == right_max {
            return left_min <= right_min && right_min < left_max;
        }
        left_min < right_max && right_min < left_max
    }

    fn check_normalize(input: Vec<Selection>, primary: usize) -> Result<(), TestCaseError> {
        let result = Selections::new(input.clone(), primary);
        let output: Vec<Selection> = result.iter().copied().collect();
        prop_assert!(!output.is_empty());
        for pair in output.windows(2) {
            prop_assert!(pair[0].min() < pair[1].min());
            prop_assert!(!ranges_overlap(pair[0], pair[1]));
        }
        let current = result.primary();
        prop_assert!(output.contains(&current));
        if input.is_empty() {
            prop_assert_eq!(output.len(), 1);
            prop_assert_eq!(current, Selection::caret(Offset(0)));
            return Ok(());
        }
        let index = primary.min(input.len() - 1);
        let origin = input[index];
        prop_assert!(current.min() <= origin.min());
        prop_assert!(current.max() >= origin.max());
        prop_assert_eq!(current.goal_col, origin.goal_col);
        prop_assert_eq!(current.head < current.anchor, origin.head < origin.anchor);
        for selection in &input {
            let covered = output
                .iter()
                .any(|out| out.min() <= selection.min() && out.max() >= selection.max());
            prop_assert!(covered);
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn normalize_sorts_merges_and_keeps_primary((input, primary) in arb_case()) {
            check_normalize(input, primary)?;
        }
    }

    #[test]
    fn primary_past_the_end_does_not_panic() {
        let sel = Selections::new(vec![Selection::caret(Offset(3))], usize::MAX);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.primary(), Selection::caret(Offset(3)));
        let empty = Selections::new(vec![], usize::MAX);
        assert_eq!(empty.primary(), Selection::caret(Offset(0)));
    }

    #[test]
    fn push_becomes_primary_and_merges_overlap() {
        let mut sel = Selections::single(Selection {
            anchor: Offset(0),
            head: Offset(4),
            goal_col: Some(2),
        });
        sel.push(Selection {
            anchor: Offset(6),
            head: Offset(2),
            goal_col: Some(8),
        });
        assert_eq!(sel.len(), 1);
        assert_eq!(
            sel.primary(),
            Selection {
                anchor: Offset(6),
                head: Offset(0),
                goal_col: Some(8),
            }
        );
    }

    #[test]
    fn remove_secondary_keeps_the_primary_only() {
        let mut sel = Selections::new(
            vec![
                Selection::caret(Offset(1)),
                Selection::caret(Offset(4)),
                Selection::caret(Offset(8)),
            ],
            1,
        );
        sel.remove_secondary();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.primary(), Selection::caret(Offset(4)));
    }

    #[test]
    fn transformed_uses_left_anchor_and_right_head() {
        let sel = Selections::single(Selection::caret(Offset(1)));
        let tx = Transaction::single(Edit::insert(Offset(1), "XY"));
        let after = sel.transformed(&tx);
        assert_eq!(after.len(), 1);
        assert_eq!(after.primary().anchor, Offset(1));
        assert_eq!(after.primary().head, Offset(3));
        assert!(!after.primary().is_caret());
    }
}
