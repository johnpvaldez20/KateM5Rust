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

/// Next line to color after a plain cache miss.
///
/// `color_at` is the next line to color.
/// `visible_first..visible_last` is the viewport. `visible_last` is exclusive.
/// `plain_line` is the first visible line drawn with no theme spans.
///
/// A plain line before an in-viewport cursor is a hole. Coloring resumes at that hole.
/// A plain line at or after that cursor is the tail. The cursor stays.
/// A cursor outside the viewport resumes at `visible_first`.
/// Lines above a later miss can be plain cache hits, and they still need color.
pub(crate) fn resume_color_line(
    color_at: u32,
    visible_first: u32,
    visible_last: u32,
    plain_line: u32,
) -> u32 {
    let cursor_in_view = (visible_first..visible_last).contains(&color_at);
    if !cursor_in_view {
        visible_first
    } else if plain_line < color_at {
        plain_line.max(visible_first)
    } else {
        color_at
    }
}

/// `checkpoint` is the first line that has not been highlighted yet.
/// `first` and `last` are inclusive viewport lines.
pub(crate) fn highlight_step(checkpoint: u32, first: u32, last: u32, budget: u32) -> HighlightStep {
    if last < first {
        return HighlightStep {
            ensure_through: None,
            plain_from: Some(first),
        };
    }
    if budget == 0 && last < checkpoint {
        return HighlightStep {
            ensure_through: None,
            plain_from: None,
        };
    }
    if budget == 0 && first < checkpoint {
        return HighlightStep {
            ensure_through: None,
            plain_from: Some(checkpoint),
        };
    }
    if budget == 0 {
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
    use super::{PAINT_LINE_BUDGET, highlight_step, resume_color_line};

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

    #[test]
    fn resume_color_line_keeps_the_cursor_when_the_plain_line_is_later() {
        // Viewport is half-open. The cursor is line 24. The plain miss is line 30.
        assert_eq!(resume_color_line(24, 0, 40, 30), 24);
    }

    #[test]
    fn resume_color_line_moves_to_a_plain_line_before_the_cursor() {
        assert_eq!(resume_color_line(24, 0, 40, 10), 10);
    }

    #[test]
    fn resume_color_line_starts_at_the_first_line_when_the_cursor_is_outside() {
        // `40` is the exclusive end, so the cursor is outside `0..40`.
        // Line 20 is the first plain miss. Lines above it can still be plain cache hits.
        assert_eq!(resume_color_line(40, 0, 40, 20), 0);
        assert_eq!(resume_color_line(640, 10, 50, 40), 10);
    }

    #[test]
    fn resume_color_line_clamps_a_hole_to_the_viewport() {
        // Hole at line 0, viewport 10..50, cursor at 20 (inside).
        // The hole is before the viewport, so clamp to visible_first.
        assert_eq!(resume_color_line(20, 10, 50, 0), 10);
        // Hole at line 10, viewport 0..40, cursor at 24 (inside).
        // The hole is inside the viewport, so it stays.
        assert_eq!(resume_color_line(24, 0, 40, 10), 10);
        // Plain line at 30 is at or after cursor 24; cursor stays.
        assert_eq!(resume_color_line(24, 0, 40, 30), 24);
        // Cursor at the exclusive end 40 is outside 0..40; return visible_first.
        assert_eq!(resume_color_line(40, 0, 40, 20), 0);
    }

    #[test]
    fn a_zero_budget_keeps_color_behind_the_checkpoint() {
        // Viewport is behind the checkpoint: lines are already colored.
        let step = highlight_step(1000, 10, 50, 0);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, None);

        // Viewport at the checkpoint with zero budget: lines still need color.
        let step = highlight_step(0, 0, 39, 0);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, Some(0));

        // Empty viewport (last < first) still returns plain_from Some(first).
        let step = highlight_step(0, 10, 5, 0);
        assert_eq!(step.plain_from, Some(10));
    }

    #[test]
    fn a_zero_budget_starts_plain_at_the_checkpoint() {
        // Viewport straddles the checkpoint: 10..=150 with checkpoint at 100.
        // Zero budget means we cannot walk any lines, so color stays before
        // the checkpoint and plain starts there.
        let step = highlight_step(100, 10, 150, 0);
        assert_eq!(step.ensure_through, None);
        assert_eq!(step.plain_from, Some(100));
    }
}
