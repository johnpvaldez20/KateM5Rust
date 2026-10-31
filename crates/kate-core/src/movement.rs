// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Grapheme movement for one selection and for the whole set.
//!
//! Left and right step by extended grapheme cluster.
//! Vertical movement uses logical lines and a tab column.
//! Word movement follows Kate with camel-cursor off.

use kate_text::TextBuffer;

use crate::selection::{Selection, Selections};

/// A cursor motion.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Movement {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    SmartHome,
    DocStart,
    DocEnd,
    Lines(i32),
}

/// `Yes` keeps `anchor` and moves `head`. `No` makes a caret.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Extend {
    No,
    Yes,
}

/// Settings for movement. `tab_width` expands tab columns.
///
/// A width of 0 is treated as 1 so the move does not divide by zero.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MoveContext {
    pub tab_width: u8,
}

/// Move `s` by `m`.
///
/// The move starts at `head`. A bad offset is clamped to the buffer.
pub fn move_selection(
    buf: &TextBuffer,
    s: Selection,
    m: Movement,
    extend: Extend,
    ctx: &MoveContext,
) -> Selection {
    // Stub. The real move lands in the next commit.
    let _ = (buf, m, extend, ctx);
    s
}

/// Move every selection by `m`. Then normalize the set.
pub fn move_all(
    buf: &TextBuffer,
    sel: &mut Selections,
    m: Movement,
    extend: Extend,
    ctx: &MoveContext,
) {
    let _ = (buf, m, extend, ctx);
    let _ = sel.primary();
}

#[cfg(test)]
mod tests {
    use super::{Extend, MoveContext, Movement, move_all, move_selection};
    use crate::selection::{Selection, Selections};
    use kate_text::{CharRange, LineCol, Offset, TextBuffer};

    fn ctx() -> MoveContext {
        MoveContext { tab_width: 4 }
    }

    #[test]
    fn right_steps_over_grapheme_clusters() {
        let text = "e\u{301}x";
        assert_eq!(text.chars().count(), 3);
        let buf = TextBuffer::from_str(text);
        let moved = move_selection(
            &buf,
            Selection {
                anchor: Offset(0),
                head: Offset(0),
                goal_col: Some(4),
            },
            Movement::Right,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(2));
        assert!(moved.is_caret());
        assert_eq!(moved.goal_col, None);

        let back = move_selection(
            &buf,
            Selection::caret(Offset(2)),
            Movement::Left,
            Extend::No,
            &ctx(),
        );
        assert_eq!(back.head, Offset(0));

        let extended = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::Right,
            Extend::Yes,
            &ctx(),
        );
        assert_eq!(extended.anchor, Offset(0));
        assert_eq!(extended.head, Offset(2));

        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        assert_eq!(family.chars().count(), 7);
        let buf = TextBuffer::from_str(family);
        let moved = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::Right,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(7));
    }

    #[test]
    fn vertical_move_keeps_goal_column_across_short_lines() {
        let text = "0123456789abc\n\n\tabcdefghij";
        let buf = TextBuffer::from_str(text);
        assert_eq!(
            buf.slice(CharRange::new(Offset(22), Offset(23)))
                .to_string(),
            "g"
        );
        let start = Selection::caret(Offset(10));
        let mid = move_selection(&buf, start, Movement::Down, Extend::No, &ctx());
        assert_eq!(mid.head, Offset(14));
        assert_eq!(mid.goal_col, Some(10));
        assert!(mid.is_caret());
        let end = move_selection(&buf, mid, Movement::Down, Extend::No, &ctx());
        assert_eq!(end.head, Offset(22));
        assert_eq!(end.goal_col, Some(10));
        assert!(end.is_caret());

        let jumped = move_selection(&buf, start, Movement::Lines(2), Extend::No, &ctx());
        assert_eq!(jumped.head, Offset(22));
        assert_eq!(jumped.goal_col, Some(10));
    }

    #[test]
    fn word_movement_follows_kate_rules() {
        let buf = TextBuffer::from_str("foo bar quzzi");
        let mut selection = Selection::caret(Offset(0));
        for col in [4u32, 8, 13] {
            selection = move_selection(&buf, selection, Movement::WordRight, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head).col, col);
        }

        let buf = TextBuffer::from_str("  -  1234  xyz");
        let mut selection = Selection::caret(Offset(1));
        for col in [2u32, 5, 11] {
            selection = move_selection(&buf, selection, Movement::WordRight, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head).col, col);
        }

        let buf = TextBuffer::from_str("foo bar quzzi");
        let mut selection = Selection::caret(Offset(8));
        for col in [4u32, 0] {
            selection = move_selection(&buf, selection, Movement::WordLeft, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head).col, col);
        }

        let buf = TextBuffer::from_str("  12  -  ");
        let mut selection = Selection::caret(Offset(8));
        for col in [6u32, 2] {
            selection = move_selection(&buf, selection, Movement::WordLeft, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head).col, col);
        }

        let buf = TextBuffer::from_str("hello  there...\n\tno  one answers.");
        let mut selection = Selection::caret(Offset(0));
        let right = [
            LineCol { line: 0, col: 7 },
            LineCol { line: 0, col: 12 },
            LineCol { line: 0, col: 15 },
            LineCol { line: 1, col: 1 },
            LineCol { line: 1, col: 5 },
            LineCol { line: 1, col: 9 },
            LineCol { line: 1, col: 16 },
        ];
        for expected in right {
            selection = move_selection(&buf, selection, Movement::WordRight, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head), expected);
        }
        let left = [
            LineCol { line: 1, col: 9 },
            LineCol { line: 1, col: 5 },
            LineCol { line: 1, col: 1 },
            LineCol { line: 0, col: 15 },
            LineCol { line: 0, col: 12 },
        ];
        for expected in left {
            selection = move_selection(&buf, selection, Movement::WordLeft, Extend::No, &ctx());
            assert_eq!(buf.line_col(selection.head), expected);
        }

        let buf = TextBuffer::from_str("$ foo");
        let selection = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::WordRight,
            Extend::No,
            &ctx(),
        );
        assert_eq!(selection.head, Offset(1));

        let buf = TextBuffer::from_str("a-b");
        let selection = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::WordRight,
            Extend::No,
            &ctx(),
        );
        assert_eq!(selection.head, Offset(1));

        let buf = TextBuffer::from_str("HelloWorld");
        let selection = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::WordRight,
            Extend::No,
            &ctx(),
        );
        assert_eq!(selection.head, Offset(10));

        let buf = TextBuffer::from_str("foo_Bar");
        let selection = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::WordRight,
            Extend::No,
            &ctx(),
        );
        assert_eq!(selection.head, Offset(7));
    }

    #[test]
    fn smart_home_toggles_indent_and_column_zero() {
        let buf = TextBuffer::from_str("   abc");
        let at_text = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::SmartHome,
            Extend::No,
            &ctx(),
        );
        assert_eq!(at_text.head, Offset(3));
        let at_zero = move_selection(&buf, at_text, Movement::SmartHome, Extend::No, &ctx());
        assert_eq!(at_zero.head, Offset(0));

        let blank = TextBuffer::from_str("   ");
        let home = move_selection(
            &blank,
            Selection::caret(Offset(2)),
            Movement::SmartHome,
            Extend::No,
            &ctx(),
        );
        assert_eq!(home.head, Offset(0));
    }

    #[test]
    fn horizontal_move_crosses_a_line_break() {
        let buf = TextBuffer::from_str("ab\ncd");
        let moved = move_selection(
            &buf,
            Selection::caret(Offset(2)),
            Movement::Right,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(3));
        let back = move_selection(&buf, moved, Movement::Left, Extend::No, &ctx());
        assert_eq!(back.head, Offset(2));

        let buf = TextBuffer::from_str("a\r\nb");
        let moved = move_selection(
            &buf,
            Selection::caret(Offset(1)),
            Movement::Right,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(3));
        let back = move_selection(
            &buf,
            Selection::caret(Offset(3)),
            Movement::Left,
            Extend::No,
            &ctx(),
        );
        assert_eq!(back.head, Offset(1));
    }

    #[test]
    fn move_all_merges_carets_that_land_together() {
        let buf = TextBuffer::from_str("a");
        let mut sel = Selections::new(
            vec![Selection::caret(Offset(0)), Selection::caret(Offset(1))],
            0,
        );
        move_all(&buf, &mut sel, Movement::DocEnd, Extend::No, &ctx());
        assert_eq!(sel.len(), 1);
        assert_eq!(sel.primary().head, buf.len());
    }

    #[test]
    fn bad_offsets_do_not_panic() {
        let buf = TextBuffer::from_str("ab");
        let selection = Selection {
            anchor: Offset(50),
            head: Offset(80),
            goal_col: Some(3),
        };
        let moved = move_selection(&buf, selection, Movement::Right, Extend::No, &ctx());
        assert!(moved.head.0 <= buf.len().0);
        let up = move_selection(&buf, selection, Movement::Up, Extend::Yes, &ctx());
        assert!(up.head.0 <= buf.len().0);
        assert!(up.anchor.0 <= buf.len().0);
    }
}
