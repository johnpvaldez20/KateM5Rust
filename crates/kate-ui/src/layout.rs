// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lines that intersect the viewport.

use std::ops::Range;

use gpui::Pixels;
use kate_core::Scroll;

/// Return the half-open line range that intersects the viewport.
///
/// `scroll.first_line` is the first line. A partial line at the bottom is included.
/// `scroll.first_col` does not change the range.
pub(crate) fn visible_line_range(
    viewport_height: Pixels,
    line_height: Pixels,
    scroll: Scroll,
    line_count: u32,
) -> Range<u32> {
    if line_count == 0 || line_height <= Pixels::ZERO || viewport_height <= Pixels::ZERO {
        return 0..0;
    }
    let start = scroll.first_line.min(line_count);
    let ratio: f32 = viewport_height / line_height;
    if !ratio.is_finite() || ratio <= 0.0 {
        return start..start;
    }
    let rows = ratio.ceil();
    if !rows.is_finite() || rows <= 0.0 {
        return start..start;
    }
    let count = if rows >= u32::MAX as f32 {
        u32::MAX
    } else {
        rows as u32
    };
    if count == 0 {
        return start..start;
    }
    start..start.saturating_add(count).min(line_count)
}

#[cfg(test)]
mod tests {
    use super::visible_line_range;
    use crate::ShapedLineCache;
    use gpui::{ShapedLine, px};
    use kate_core::Scroll;
    use kate_text::Revision;

    fn scroll(first_line: u32, first_col: u32) -> Scroll {
        Scroll {
            first_line,
            first_col,
        }
    }

    #[test]
    fn visible_line_range_matches_viewport_and_scroll() {
        let line_height = px(20.0);
        assert_eq!(
            visible_line_range(px(100.0), line_height, scroll(0, 0), 50),
            0..5
        );
        assert_eq!(
            visible_line_range(px(101.0), line_height, scroll(2, 9), 50),
            2..8
        );
        assert_eq!(
            visible_line_range(px(1.0), line_height, scroll(0, 0), 10),
            0..1
        );
        assert_eq!(
            visible_line_range(px(100.0), line_height, scroll(98, 0), 100),
            98..100
        );
        assert_eq!(
            visible_line_range(px(100.0), line_height, scroll(100, 0), 100),
            100..100
        );
        assert_eq!(
            visible_line_range(px(100.0), line_height, scroll(150, 4), 100),
            100..100
        );
        assert_eq!(
            visible_line_range(px(0.0), line_height, scroll(0, 0), 10),
            0..0
        );
        assert_eq!(
            visible_line_range(px(100.0), px(0.0), scroll(0, 0), 10),
            0..0
        );
    }

    #[test]
    fn shaped_line_cache_invalidates_on_revision_change() {
        let mut cache = ShapedLineCache {
            lines: Default::default(),
        };
        let revision = Revision(1);
        cache.insert(4, revision, ShapedLine::default());
        assert!(
            cache.get(4, revision).is_some(),
            "same line and revision is a hit"
        );
        assert!(
            cache.get(4, Revision(2)).is_none(),
            "a new buffer revision is a miss"
        );
        assert!(cache.get(5, revision).is_none(), "another line is a miss");
    }
}
