// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

//! How far a paint may highlight.
//!
//! A scroll that jumps ahead does not highlight the skipped lines inside paint.
//! The paint draws those lines as plain text.
//! A later turn continues from the checkpoint.

/// Lines `ensure` may cover inside one paint.
pub(crate) const PAINT_LINE_BUDGET: u32 = 128;

/// Lines one catch-up turn may cover outside paint.
pub(crate) const CATCH_UP_LINE_BUDGET: u32 = 512;

/// Lines one paint may recolor after they were drawn plain.
///
/// A full viewport of theme spans is too slow for one frame.
pub(crate) const COLOR_LINE_BUDGET: u32 = 12;

/// What one paint should do with the syntax cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HighlightStep {
    /// Inclusive line to pass to `ensure`. `None` means do not call it.
    pub ensure_through: Option<u32>,
    /// Visible lines at this index and after are drawn with no syntax colors.
    pub plain_from: Option<u32>,
}

/// `checkpoint` is the first line that has not been highlighted yet.
/// `first` and `last` are inclusive viewport lines.
pub(crate) fn highlight_step(checkpoint: u32, first: u32, last: u32, budget: u32) -> HighlightStep {
    if last < first || budget == 0 {
        return HighlightStep {
            ensure_through: None,
            plain_from: Some(first),
        };
    }
    if last < checkpoint {
        return HighlightStep {
            ensure_through: None,
            plain_from: None,
        };
    }
    // Any line before `first` was skipped. Do not walk it inside paint.
    if first > checkpoint {
        return HighlightStep {
            ensure_through: None,
            plain_from: Some(first),
        };
    }
    let budget_end = checkpoint.saturating_add(budget.saturating_sub(1));
    if first > budget_end {
        return HighlightStep {
            ensure_through: None,
            plain_from: Some(first),
        };
    }
    let through = last.min(budget_end);
    let plain_from = if through < last {
        Some(through.saturating_add(1))
    } else {
        None
    };
    HighlightStep {
        ensure_through: Some(through),
        plain_from,
    }
}

#[cfg(test)]
mod tests {
    use super::{PAINT_LINE_BUDGET, highlight_step};

    #[test]
    fn highlight_step_covers_a_viewport_at_the_checkpoint() {
        let step = highlight_step(0, 0, 39, PAINT_LINE_BUDGET);
        assert_eq!(step.ensure_through, Some(39));
        assert_eq!(step.plain_from, None);
    }

    #[test]
    fn highlight_step_does_not_walk_a_skipped_gap() {
        let step = highlight_step(0, 90_000, 90_040, PAINT_LINE_BUDGET);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, Some(90_000));
    }

    #[test]
    fn highlight_step_does_not_walk_a_short_gap() {
        let step = highlight_step(100, 168, 208, PAINT_LINE_BUDGET);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, Some(168));
    }

    #[test]
    fn highlight_step_stops_at_the_budget_inside_a_long_viewport() {
        let step = highlight_step(0, 0, 400, 128);
        assert_eq!(step.ensure_through, Some(127));
        assert_eq!(step.plain_from, Some(128));
    }

    #[test]
    fn highlight_step_leaves_a_region_behind_the_checkpoint() {
        let step = highlight_step(1_000, 10, 50, PAINT_LINE_BUDGET);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, None);
    }
}
