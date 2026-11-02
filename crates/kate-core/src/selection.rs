// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! A sorted set of selections.
//!
//! One cursor is a set of size one.
//! The set stays sorted by the minimum offset.
//! Ranges do not overlap.
//! The primary index always points at one range.

use kate_text::{Bias, CharRange, Edit, Offset, Transaction, transform_offset};

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
        normalize(ranges, primary)
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
        let mut ranges = std::mem::take(&mut self.ranges);
        ranges.push(s);
        let primary = ranges.len() - 1;
        *self = Self::new(ranges, primary);
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
        let primary = self.primary;
        let ranges = self.ranges.iter().copied().map(&mut f).collect();
        *self = Self::new(ranges, primary);
    }

    /// Map every end through `tx`. Then normalize.
    ///
    /// The walk is one sorted pass over the selections and the edits.
    /// The minimum end uses [`Bias::Left`]. The maximum end uses [`Bias::Right`].
    /// A forward range and a backward range keep the same span.
    /// The direction of the selection stays as it was.
    /// When one edit covers a selection exactly, that edit is the only bias source.
    /// Earlier edits then add one running delta. `goal_col` stays as it is.
    pub fn transformed(&self, tx: &Transaction) -> Selections {
        let edits = tx.edits();
        let mut walk = EditWalk::new(edits);
        let ranges = self
            .ranges
            .iter()
            .map(|selection| walk.map_selection(selection))
            .collect();
        Self::new(ranges, self.primary)
    }
}

struct Prepared {
    left_final: Vec<Offset>,
    right_final: Vec<Offset>,
    /// `prefix[i]` is the sum of deltas of edits before `i`.
    prefix: Vec<isize>,
}

impl Prepared {
    fn new(edits: &[Edit]) -> Self {
        let mut prefix = Vec::with_capacity(edits.len() + 1);
        prefix.push(0);
        let mut sum = 0isize;
        for edit in edits {
            sum = add_delta(sum, edit.delta());
            prefix.push(sum);
        }
        let mut left_final = vec![Offset(0); edits.len()];
        let mut right_final = vec![Offset(0); edits.len()];
        for (index, edit) in edits.iter().enumerate() {
            let start = edit.start.0;
            if index > 0 && edits[index - 1].old_end().0 == start {
                left_final[index] = left_final[index - 1];
            } else {
                left_final[index] = shift_point(Offset(start), prefix[index]);
            }
            let landing = start.saturating_add(edit.new.chars().count());
            if index > 0 && landing == edits[index - 1].old_end().0 {
                right_final[index] = right_final[index - 1];
            } else {
                right_final[index] = shift_point(Offset(landing), prefix[index]);
            }
        }
        Self {
            left_final,
            right_final,
            prefix,
        }
    }
}

struct EditWalk<'a> {
    index: usize,
    edits: &'a [Edit],
    prepared: Prepared,
}

impl<'a> EditWalk<'a> {
    fn new(edits: &'a [Edit]) -> Self {
        Self {
            index: 0,
            prepared: Prepared::new(edits),
            edits,
        }
    }

    fn map_selection(&mut self, selection: &Selection) -> Selection {
        let min = selection.min().0;
        let max = selection.max().0;
        while self.index < self.edits.len() && self.edits[self.index].old_end().0 < min {
            self.index += 1;
        }
        if let Some(cover) = self.exact_cover_at(min, max) {
            return map_exact(selection, &self.edits[cover], self.prepared.prefix[cover]);
        }
        let at_min = self.map_point(min, Bias::Left);
        let at_max = self.map_point(max, Bias::Right);
        if selection.anchor.0 <= selection.head.0 {
            Selection {
                anchor: at_min,
                head: at_max,
                goal_col: selection.goal_col,
            }
        } else {
            Selection {
                anchor: at_max,
                head: at_min,
                goal_col: selection.goal_col,
            }
        }
    }

    fn exact_cover_at(&self, min: usize, max: usize) -> Option<usize> {
        let mut index = self.index;
        if index < self.edits.len() && self.edits[index].start.0 < min {
            index += 1;
        }
        if index < self.edits.len() && exact_cover(&self.edits[index], min, max) {
            Some(index)
        } else {
            None
        }
    }

    fn map_point(&mut self, offset: usize, bias: Bias) -> Offset {
        while self.index < self.edits.len() && self.edits[self.index].old_end().0 < offset {
            self.index += 1;
        }
        let start = self.index;
        let mut end = start;
        while end < self.edits.len() && contains_offset(&self.edits[end], offset) {
            end += 1;
        }
        if start == end {
            return shift_point(Offset(offset), self.prepared.prefix[self.index]);
        }
        if end == start + 1 {
            return if bias == Bias::Left {
                self.prepared.left_final[start]
            } else {
                self.prepared.right_final[start]
            };
        }
        let mut at = Offset(offset);
        for edit in self.edits[start..end].iter().rev() {
            at = transform_offset(at, bias, edit);
        }
        self.resolve_before(at.0, bias, start)
    }

    fn resolve_before(&self, offset: usize, bias: Bias, before: usize) -> Offset {
        if before == 0 {
            return Offset(offset);
        }
        let mut index = before;
        while index > 0 && self.edits[index - 1].start.0 > offset {
            index -= 1;
        }
        if index == 0 {
            return Offset(offset);
        }
        if !contains_offset(&self.edits[index - 1], offset) {
            return shift_point(Offset(offset), self.prepared.prefix[index]);
        }
        let mut start = index - 1;
        while start > 0 && contains_offset(&self.edits[start - 1], offset) {
            start -= 1;
        }
        if index - start == 1 {
            return if bias == Bias::Left {
                self.prepared.left_final[start]
            } else {
                self.prepared.right_final[start]
            };
        }
        let mut at = Offset(offset);
        for edit in self.edits[start..index].iter().rev() {
            at = transform_offset(at, bias, edit);
        }
        self.resolve_before(at.0, bias, start)
    }
}

fn contains_offset(edit: &Edit, offset: usize) -> bool {
    offset >= edit.start.0 && offset <= edit.old_end().0
}

fn map_exact(selection: &Selection, edit: &Edit, delta: isize) -> Selection {
    let start = shift_point(transform_offset(selection.min(), Bias::Left, edit), delta);
    let end = shift_point(transform_offset(selection.max(), Bias::Right, edit), delta);
    if selection.anchor <= selection.head {
        Selection {
            anchor: start,
            head: end,
            goal_col: selection.goal_col,
        }
    } else {
        Selection {
            anchor: end,
            head: start,
            goal_col: selection.goal_col,
        }
    }
}

fn exact_cover(edit: &Edit, min: usize, max: usize) -> bool {
    edit.start.0 == min && edit.old_end().0 == max
}

fn add_delta(delta: isize, extra: isize) -> isize {
    delta.saturating_add(extra)
}

fn shift_point(offset: Offset, delta: isize) -> Offset {
    if delta > 0 {
        Offset(offset.0.saturating_add(delta as usize))
    } else if delta < 0 {
        Offset(offset.0.saturating_sub(delta.unsigned_abs()))
    } else {
        offset
    }
}

fn normalize(ranges: Vec<Selection>, primary: usize) -> Selections {
    if ranges.is_empty() {
        return Selections {
            ranges: vec![Selection::caret(Offset(0))],
            primary: 0,
        };
    }
    let primary = primary.min(ranges.len() - 1);
    let order = sort_indices(&ranges);
    assemble(&ranges, &order, primary)
}

fn sort_indices(items: &[Selection]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&left, &right| {
        items[left]
            .min()
            .cmp(&items[right].min())
            .then(items[left].max().cmp(&items[right].max()))
            .then(left.cmp(&right))
    });
    order
}

struct Built {
    selection: Selection,
    owns_primary: bool,
}

fn assemble(items: &[Selection], order: &[usize], primary: usize) -> Selections {
    let mut built: Vec<Built> = Vec::new();
    for &index in order {
        let selection = items[index];
        if absorb_into_last(&mut built, selection, index == primary) {
            continue;
        }
        built.push(Built {
            selection,
            owns_primary: index == primary,
        });
    }
    let primary_at = built.iter().position(|item| item.owns_primary).unwrap_or(0);
    Selections {
        ranges: built.into_iter().map(|item| item.selection).collect(),
        primary: primary_at,
    }
}

fn absorb_into_last(built: &mut [Built], selection: Selection, is_primary: bool) -> bool {
    let Some(last) = built.last_mut() else {
        return false;
    };
    if !overlaps(&last.selection, &selection) {
        return false;
    }
    let current = last.selection;
    let owner = if is_primary && !last.owns_primary {
        selection
    } else {
        current
    };
    last.selection = union_selection(owner, &current, &selection);
    last.owns_primary = last.owns_primary || is_primary;
    true
}

fn union_selection(owner: Selection, left: &Selection, right: &Selection) -> Selection {
    let min = left.min().min(right.min());
    let max = left.max().max(right.max());
    if owner.head < owner.anchor {
        Selection {
            anchor: max,
            head: min,
            goal_col: owner.goal_col,
        }
    } else {
        Selection {
            anchor: min,
            head: max,
            goal_col: owner.goal_col,
        }
    }
}

struct Span {
    start: usize,
    end: usize,
}

fn span_of(selection: &Selection) -> Span {
    Span {
        start: selection.min().0,
        end: selection.max().0,
    }
}

fn overlaps(left: &Selection, right: &Selection) -> bool {
    let left = span_of(left);
    let right = span_of(right);
    if left.start == left.end && right.start == right.end {
        return left.start == right.start;
    }
    if left.start == left.end {
        return point_inside(left.start, &right);
    }
    if right.start == right.end {
        return point_inside(right.start, &left);
    }
    left.start < right.end && right.start < left.end
}

fn point_inside(point: usize, span: &Span) -> bool {
    span.start <= point && point < span.end
}

#[cfg(test)]
mod tests {
    use super::{Selection, Selections};
    use kate_text::{Bias, Edit, Offset, Transaction, transform_offset};
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
    fn insert_at_both_ends_grows_forward_and_backward_ranges() {
        let tx = Transaction::new(vec![
            Edit::insert(Offset(2), "é"),
            Edit::insert(Offset(6), "XY"),
        ])
        .expect("the two inserts do not overlap");
        let forward = Selection {
            anchor: Offset(2),
            head: Offset(6),
            goal_col: Some(3),
        };
        let backward = Selection {
            anchor: Offset(6),
            head: Offset(2),
            goal_col: Some(9),
        };
        let grown_forward = Selections::single(forward).transformed(&tx).primary();
        let grown_backward = Selections::single(backward).transformed(&tx).primary();
        assert_eq!(grown_forward.min(), Offset(2));
        assert_eq!(grown_forward.max(), Offset(9));
        assert_eq!(grown_backward.min(), grown_forward.min());
        assert_eq!(grown_backward.max(), grown_forward.max());
        assert!(grown_forward.anchor <= grown_forward.head);
        assert!(grown_backward.head < grown_backward.anchor);
        assert_eq!(grown_forward.goal_col, Some(3));
        assert_eq!(grown_backward.goal_col, Some(9));
    }

    #[test]
    fn exact_cover_keeps_the_same_span_in_both_directions() {
        let edit = Edit {
            start: Offset(2),
            old: "abcd".to_string(),
            new: "é".to_string(),
        };
        let tx = Transaction::single(edit);
        let forward = Selection {
            anchor: Offset(2),
            head: Offset(6),
            goal_col: Some(1),
        };
        let backward = Selection {
            anchor: Offset(6),
            head: Offset(2),
            goal_col: Some(2),
        };
        let forward_after = Selections::single(forward).transformed(&tx).primary();
        let backward_after = Selections::single(backward).transformed(&tx).primary();
        assert_eq!(forward_after.min(), Offset(2));
        assert_eq!(forward_after.max(), Offset(3));
        assert_eq!(backward_after.min(), forward_after.min());
        assert_eq!(backward_after.max(), forward_after.max());
        assert!(forward_after.anchor <= forward_after.head);
        assert!(backward_after.head < backward_after.anchor);
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

    /// Left bias on the minimum end. Right bias on the maximum end.
    ///
    /// A forward range and a backward range use the same span rule.
    fn end_bias(selection: &Selection) -> (Bias, Bias) {
        if selection.anchor.0 <= selection.head.0 {
            (Bias::Left, Bias::Right)
        } else {
            (Bias::Right, Bias::Left)
        }
    }

    fn oracle_point(offset: Offset, bias: Bias, tx: &Transaction) -> Offset {
        let mut at = offset;
        for edit in tx.edits().iter().rev() {
            at = transform_offset(at, bias, edit);
        }
        at
    }

    fn covers_selection(selection: &Selection, tx: &Transaction) -> bool {
        let min = selection.min().0;
        let max = selection.max().0;
        tx.edits()
            .iter()
            .any(|edit| edit.start.0 == min && edit.old_end().0 == max)
    }

    #[test]
    fn transformed_maps_edits_that_do_not_cover_the_selection() {
        let range = Selection {
            anchor: Offset(2),
            head: Offset(5),
            goal_col: None,
        };
        let inserted = Selections::single(range)
            .transformed(&Transaction::single(Edit::insert(Offset(2), "ab")));
        assert_eq!(inserted.primary().anchor, Offset(2));
        assert_eq!(inserted.primary().head, Offset(7));

        let shrunk = Selections::single(Selection {
            anchor: Offset(0),
            head: Offset(10),
            goal_col: None,
        })
        .transformed(&Transaction::single(Edit::delete(Offset(4), "xx")));
        assert_eq!(shrunk.primary().anchor, Offset(0));
        assert_eq!(shrunk.primary().head, Offset(8));

        let trimmed = Selections::single(Selection {
            anchor: Offset(2),
            head: Offset(6),
            goal_col: None,
        })
        .transformed(&Transaction::single(Edit::delete(Offset(4), "yz")));
        assert_eq!(trimmed.primary().anchor, Offset(2));
        assert_eq!(trimmed.primary().head, Offset(4));

        let carets = Selections::new(
            vec![Selection::caret(Offset(3)), Selection::caret(Offset(7))],
            0,
        )
        .transformed(&Transaction::single(Edit::delete(Offset(2), "wxyz")));
        assert_eq!(carets.len(), 2);
        let heads: Vec<Offset> = carets.iter().map(|selection| selection.head).collect();
        assert_eq!(heads, vec![Offset(2), Offset(3)]);

        let mixed = Selections::new(
            vec![
                Selection {
                    anchor: Offset(2),
                    head: Offset(6),
                    goal_col: None,
                },
                Selection::caret(Offset(8)),
            ],
            1,
        )
        .transformed(&Transaction::single(Edit::delete(Offset(1), "abc")));
        assert_eq!(mixed.len(), 2);
        let ends: Vec<(Offset, Offset)> = mixed
            .iter()
            .map(|selection| (selection.anchor, selection.head))
            .collect();
        assert_eq!(ends, vec![(Offset(1), Offset(3)), (Offset(5), Offset(5))]);
    }

    fn disjoint_edits(parts: Vec<(usize, usize, usize)>) -> Transaction {
        let mut cursor = 0usize;
        let mut edits = Vec::new();
        for (gap, old_len, new_len) in parts {
            cursor = cursor.saturating_add(gap % 4);
            let old: String = "abcdef".chars().cycle().take(old_len).collect();
            let fresh: String = "xyz".chars().cycle().take(new_len).collect();
            edits.push(Edit {
                start: Offset(cursor),
                old,
                new: fresh,
            });
            cursor = cursor.saturating_add(old_len);
        }
        Transaction::new(edits).expect("the generator builds disjoint edits")
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn transformed_matches_transform_offset_for_foreign_edits(
            (selections, primary) in arb_input(),
            parts in prop::collection::vec((0usize..5, 0usize..5, 0usize..4), 0..6),
        ) {
            let tx = disjoint_edits(parts);
            let base = Selections::new(selections, primary);
            let stored: Vec<Selection> = base.iter().copied().collect();
            let aligned = Selections::new(stored, 0);
            let combined = aligned.transformed(&tx);
            let parts: Vec<Selection> = aligned
                .iter()
                .copied()
                .map(|selection| Selections::single(selection).transformed(&tx).primary())
                .collect();
            let expected = Selections::new(parts, 0);
            prop_assert_eq!(combined, expected);
            for selection in aligned.iter() {
                if covers_selection(selection, &tx) {
                    continue;
                }
                let got = Selections::single(*selection).transformed(&tx).primary();
                let (anchor_bias, head_bias) = end_bias(selection);
                prop_assert_eq!(got.anchor, oracle_point(selection.anchor, anchor_bias, &tx));
                prop_assert_eq!(got.head, oracle_point(selection.head, head_bias, &tx));
                prop_assert_eq!(got.goal_col, selection.goal_col);
            }
        }

        #[test]
        fn forward_and_backward_ranges_keep_one_span(
            start in 0usize..24,
            len in 1usize..12,
            parts in prop::collection::vec((0usize..4, 0usize..4, 0usize..3), 0..5),
            left in prop::sample::select(vec!["é", "你", "Q", ""]),
            right in prop::sample::select(vec!["XY", "é", "a", ""]),
        ) {
            let end = start + len;
            let mut edits = disjoint_edits(parts).edits().to_vec();
            if !left.is_empty() {
                edits.push(Edit::insert(Offset(start), left));
            }
            if !right.is_empty() {
                edits.push(Edit::insert(Offset(end), right));
            }
            let Ok(tx) = Transaction::new(edits) else {
                return Ok(());
            };
            let forward = Selection {
                anchor: Offset(start),
                head: Offset(end),
                goal_col: Some(1),
            };
            let backward = Selection {
                anchor: Offset(end),
                head: Offset(start),
                goal_col: Some(2),
            };
            let forward_after = Selections::single(forward).transformed(&tx).primary();
            let backward_after = Selections::single(backward).transformed(&tx).primary();
            prop_assert_eq!(forward_after.min(), backward_after.min());
            prop_assert_eq!(forward_after.max(), backward_after.max());
            prop_assert_eq!(forward_after.goal_col, Some(1));
            prop_assert_eq!(backward_after.goal_col, Some(2));
            if forward_after.min() == forward_after.max() {
                prop_assert!(forward_after.is_caret());
                prop_assert!(backward_after.is_caret());
            } else {
                prop_assert!(forward_after.anchor <= forward_after.head);
                prop_assert!(backward_after.head < backward_after.anchor);
            }
        }
    }
}
