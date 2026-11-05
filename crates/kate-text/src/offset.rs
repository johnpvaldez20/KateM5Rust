// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Offsets, ranges, line positions, and revisions.
//!
//! An offset counts Unicode scalar values from the start of the buffer.
//! A range is half-open. `new` puts the two ends in order.

/// Position in the buffer counted in Unicode scalar values (ropey's native unit).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Offset(pub usize);

/// Half-open range of offsets; `start <= end` is an invariant enforced by `new`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CharRange {
    start: Offset,
    end: Offset,
}

impl CharRange {
    /// Orders the two offsets so `start <= end`.
    pub fn new(a: Offset, b: Offset) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    pub fn start(&self) -> Offset {
        self.start
    }

    pub fn end(&self) -> Offset {
        self.end
    }

    pub fn len(&self) -> usize {
        debug_assert!(self.start <= self.end);
        self.end.0 - self.start.0
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns true when `offset` is in `[start, end)`.
    pub fn contains(&self, offset: Offset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns true when two half-open ranges share at least one scalar.
    pub fn intersects(&self, other: CharRange) -> bool {
        self.start.max(other.start) < self.end.min(other.end)
    }
}

/// Derived view of an Offset. `col` is in chars from line start.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// Monotonic edit counter. 0 for a new or freshly loaded buffer. Never decreases, not even on undo.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Revision(pub u64);

#[cfg(test)]
mod tests {
    use super::{CharRange, Offset};

    #[test]
    fn new_orders_endpoints_and_contains_is_half_open() {
        let range = CharRange::new(Offset(5), Offset(1));
        assert_eq!(range.start(), Offset(1));
        assert_eq!(range.end(), Offset(5));
        assert_eq!(range.len(), 4);
        assert!(!range.is_empty());
        assert!(range.contains(Offset(1)));
        assert!(range.contains(Offset(4)));
        assert!(!range.contains(Offset(5)));
        assert!(!range.contains(Offset(0)));

        let empty = CharRange::new(Offset(3), Offset(3));
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(!empty.contains(Offset(3)));
    }

    #[test]
    fn ranges_touch_without_overlapping() {
        // Overlapping: [1,4) and [3,5) share scalars 3.
        assert!(
            CharRange::new(Offset(1), Offset(4)).intersects(CharRange::new(Offset(3), Offset(5)))
        );
        // Touching at the end: [1,4) and [4,6) share no scalar.
        assert!(
            !CharRange::new(Offset(1), Offset(4)).intersects(CharRange::new(Offset(4), Offset(6)))
        );
        // Empty range [3,3) inside [1,4) shares no scalar.
        assert!(
            !CharRange::new(Offset(3), Offset(3)).intersects(CharRange::new(Offset(1), Offset(4)))
        );
    }
}
