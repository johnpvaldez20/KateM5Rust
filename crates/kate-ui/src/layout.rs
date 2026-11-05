// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lines that intersect the viewport.

use std::ops::Range;

use kate_core::Scroll;

#[cfg(target_os = "macos")]
use gpui::Pixels;
#[cfg(not(target_os = "macos"))]
use shim::Pixels;

#[cfg_attr(not(test), allow(dead_code))]
#[path = "notice.rs"]
mod notice;

#[cfg_attr(not(test), allow(dead_code))]
/// Return the sentence for a failed command.
///
/// An Ok result has no sentence.
pub(crate) fn notice_from(result: Result<(), kate_core::CommandError>) -> Option<String> {
    match result {
        Ok(()) => None,
        Err(error) => Some(notice::command_notice(&error)),
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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

fn is_line_break(scalar: char) -> bool {
    matches!(
        scalar,
        '\r' | '\n' | '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

#[cfg_attr(not(test), allow(dead_code))]
/// Count the visual rows for one logical line.
///
/// Each Unicode scalar has width 1. A tab advances to the next multiple of
/// `tab_width`. A `tab_width` of 0 counts as 1.
/// A line break (CR, LF, VT, FF, NEL, LS, PS) has width 0.
/// An empty line returns 1.
/// A column count of 0 returns 1.
pub(crate) fn visual_row_count(line: &str, columns: u32, tab_width: u8) -> u32 {
    if columns == 0 || line.is_empty() {
        return 1;
    }
    let step = tab_width.max(1) as u32;
    let mut width: u32 = 0;
    for scalar in line.chars() {
        if scalar == '\t' {
            width = width.saturating_add(step - (width % step));
        } else if !(is_line_break(scalar) || scalar == '\u{200B}') {
            width = width.saturating_add(1);
        }
    }
    width.div_ceil(columns).max(1)
}

#[cfg_attr(not(test), allow(dead_code))]
/// Return the half-open line range a viewport can show.
///
/// The start is `first_line` clamped to `line_count`.
/// The end is the start plus `row_count`, clamped to `line_count`.
/// A row count of 0 returns an empty range at that start.
/// A line count of 0 returns `0..0`.
pub(crate) fn span_line_window(first_line: u32, row_count: u32, line_count: u32) -> Range<u32> {
    let start = first_line.min(line_count);
    start..start.saturating_add(row_count).min(line_count)
}

#[cfg(test)]
mod tests {
    use super::{notice_from, span_line_window, visible_line_range, visual_row_count};
    use crate::{ShapedLine, ShapedLineCache};
    use kate_core::{CommandError, Scroll};
    use kate_text::Revision;

    #[cfg(not(target_os = "macos"))]
    use super::shim::px;
    #[cfg(target_os = "macos")]
    use gpui::px;

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
    fn span_line_window_clamps_to_the_buffer() {
        assert_eq!(span_line_window(10, 2, 100), 10..12);
        assert_eq!(span_line_window(98, 10, 100), 98..100);
        assert_eq!(span_line_window(10, 0, 100), 10..10);
        assert_eq!(span_line_window(150, 4, 100), 100..100);
        assert_eq!(span_line_window(4, 8, 0), 0..0);
    }

    #[test]
    fn shaped_line_cache_invalidates_on_revision_change() {
        let mut cache = ShapedLineCache {
            lines: Default::default(),
        };
        let revision = Revision(1);
        #[cfg(target_os = "macos")]
        let shaped = ShapedLine::default();
        #[cfg(not(target_os = "macos"))]
        let shaped = ShapedLine;
        cache.insert(4, revision, shaped);
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

    #[test]
    fn shaped_cache_drops_one_revision_range() {
        let mut cache = ShapedLineCache::default();
        let revision = Revision(3);
        cache.insert(1, revision, shaped_line());
        cache.insert(2, revision, shaped_line());
        cache.insert(3, revision, shaped_line());
        cache.insert(2, Revision(4), shaped_line());
        cache.invalidate_range(2, 4, revision);
        assert!(cache.get(1, revision).is_some());
        assert!(cache.get(2, revision).is_none());
        assert!(cache.get(3, revision).is_none());
        assert!(cache.get(2, Revision(4)).is_some());
    }

    #[test]
    fn visual_row_count_splits_a_long_line() {
        assert_eq!(visual_row_count("abcdef", 4, 4), 2);
        assert_eq!(visual_row_count("", 4, 4), 1);
        assert_eq!(visual_row_count("abcdef", 0, 4), 1);
        assert_eq!(visual_row_count("éééé", 4, 4), 1);
    }

    #[test]
    fn notice_from_names_a_missing_path() {
        assert_eq!(notice_from(Ok(())), None);
        assert_eq!(
            notice_from(Err(CommandError::NoFile)),
            Some("This document has no path.".to_string())
        );
    }

    fn shaped_line() -> ShapedLine {
        #[cfg(target_os = "macos")]
        {
            ShapedLine::default()
        }
        #[cfg(not(target_os = "macos"))]
        {
            ShapedLine
        }
    }

    #[test]
    fn a_tab_uses_the_tab_width() {
        assert_eq!(visual_row_count("\t", 4, 4), 1);
        assert_eq!(visual_row_count("\t\t", 4, 4), 2);
        assert_eq!(visual_row_count("a\t", 4, 4), 1);
        assert_eq!(visual_row_count("\t", 4, 0), 1);
    }

    #[test]
    fn a_carriage_return_has_no_width() {
        assert_eq!(visual_row_count("ab\rcd", 4, 4), 1);
        assert_eq!(visual_row_count("abcd", 4, 4), 1);
        assert_eq!(visual_row_count("\t", 4, 4), 1);
    }

    #[test]
    fn a_newline_has_no_width() {
        assert_eq!(visual_row_count("ab\ncd", 4, 4), 1);
        assert_eq!(visual_row_count("ab\rcd", 4, 4), 1);
    }

    #[test]
    fn a_unicode_line_break_has_no_width() {
        assert_eq!(visual_row_count("ab\u{2028}cd", 4, 4), 1);
        assert_eq!(visual_row_count("ab\u{2029}cd", 4, 4), 1);
        assert_eq!(visual_row_count("ab\ncd", 4, 4), 1);
        assert_eq!(visual_row_count("ab\rcd", 4, 4), 1);
    }

    #[test]
    fn a_zero_width_space_adds_no_row() {
        assert_eq!(visual_row_count("ab\u{200B}", 2, 4), 1);
        assert_eq!(visual_row_count("ab", 2, 4), 1);
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
mod shim {
    #[derive(Copy, Clone, PartialEq, Debug)]
    pub(crate) struct Pixels(f32);

    impl Pixels {
        pub(crate) const ZERO: Self = Self(0.0);
    }

    impl PartialOrd for Pixels {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            self.0.partial_cmp(&other.0)
        }
    }

    impl std::ops::Div for Pixels {
        type Output = f32;

        fn div(self, rhs: Self) -> f32 {
            self.0 / rhs.0
        }
    }

    pub(crate) fn px(value: f32) -> Pixels {
        Pixels(value)
    }
}
