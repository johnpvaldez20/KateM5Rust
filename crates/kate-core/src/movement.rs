// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Grapheme movement for one selection and for the whole set.
//!
//! Left and right step by extended grapheme cluster.
//! Vertical movement uses logical lines and a tab column.
//! Word movement follows Kate with camel-cursor off.

use kate_text::{CharRange, LineCol, Offset, TextBuffer};
use unicode_segmentation::UnicodeSegmentation;

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
    let head = clamp_offset(buf, s.head);
    let place = place_head(buf, head, s.goal_col, m, ctx);
    match extend {
        Extend::Yes => Selection {
            anchor: clamp_offset(buf, s.anchor),
            head: place.offset,
            goal_col: place.goal,
        },
        Extend::No => Selection {
            anchor: place.offset,
            head: place.offset,
            goal_col: place.goal,
        },
    }
}

/// Move every selection by `m`. Then normalize the set.
pub fn move_all(
    buf: &TextBuffer,
    sel: &mut Selections,
    m: Movement,
    extend: Extend,
    ctx: &MoveContext,
) {
    sel.map(|selection| move_selection(buf, selection, m, extend, ctx));
}

struct Place {
    offset: Offset,
    goal: Option<u32>,
}

fn place_head(
    buf: &TextBuffer,
    head: Offset,
    goal: Option<u32>,
    m: Movement,
    ctx: &MoveContext,
) -> Place {
    if let Some(delta) = line_delta(m) {
        return place_vertical(buf, head, goal, delta, ctx);
    }
    Place {
        offset: place_horizontal(buf, head, m),
        goal: None,
    }
}

fn line_delta(m: Movement) -> Option<i32> {
    match m {
        Movement::Up => Some(-1),
        Movement::Down => Some(1),
        Movement::Lines(count) => Some(count),
        _ => None,
    }
}

fn place_horizontal(buf: &TextBuffer, head: Offset, m: Movement) -> Offset {
    step_offset(buf, head, m)
        .or_else(|| edge_offset(buf, head, m))
        .unwrap_or(head)
}

fn step_offset(buf: &TextBuffer, head: Offset, m: Movement) -> Option<Offset> {
    match m {
        Movement::Left => Some(grapheme_before(buf, head)),
        Movement::Right => Some(grapheme_after(buf, head)),
        Movement::WordLeft => Some(word_prev(buf, head)),
        Movement::WordRight => Some(word_next(buf, head)),
        _ => None,
    }
}

fn edge_offset(buf: &TextBuffer, head: Offset, m: Movement) -> Option<Offset> {
    match m {
        Movement::LineStart => Some(line_edge(buf, head, false)),
        Movement::LineEnd => Some(line_edge(buf, head, true)),
        Movement::SmartHome => Some(smart_home(buf, head)),
        Movement::DocStart => Some(Offset(0)),
        Movement::DocEnd => Some(buf.len()),
        _ => None,
    }
}

fn place_vertical(
    buf: &TextBuffer,
    head: Offset,
    goal: Option<u32>,
    delta: i32,
    ctx: &MoveContext,
) -> Place {
    let width = tab_width(ctx);
    let current = buf.line_col(head);
    let goal_col = goal.unwrap_or_else(|| {
        let text = line_prefix(buf, current.line, current.col);
        to_virtual(&text, current.col, width)
    });
    let last = buf.line_count().saturating_sub(1);
    let target = shift_line(current.line, last, delta);
    let text = line_prefix(buf, target, goal_col);
    let col = from_virtual(&text, goal_col, width);
    let raw = buf.offset(LineCol {
        line: target,
        col: u32::try_from(col).unwrap_or(u32::MAX),
    });
    Place {
        offset: snap_floor(buf, raw),
        goal: Some(goal_col),
    }
}

fn shift_line(line: u32, last: u32, delta: i32) -> u32 {
    let target = i64::from(line) + i64::from(delta);
    let last = i64::from(last);
    u32::try_from(target.clamp(0, last)).unwrap_or(0)
}

fn tab_width(ctx: &MoveContext) -> u32 {
    u32::from(ctx.tab_width).max(1)
}

fn to_virtual(line: &str, column: u32, width: u32) -> u32 {
    let mut virtual_col = 0u32;
    let mut seen = 0u32;
    for ch in line.chars() {
        if seen == column {
            break;
        }
        virtual_col = virtual_col.saturating_add(cell_width(ch, virtual_col, width));
        seen = seen.saturating_add(1);
    }
    virtual_col.saturating_add(column.saturating_sub(seen))
}

fn from_virtual(line: &str, target: u32, width: u32) -> usize {
    if target == 0 {
        return 0;
    }
    let chars: Vec<char> = line.chars().collect();
    let maximal = chars
        .len()
        .min(usize::try_from(target).unwrap_or(usize::MAX));
    let mut virtual_col = 0u32;
    for (index, ch) in chars.iter().take(maximal).enumerate() {
        virtual_col = virtual_col.saturating_add(cell_width(*ch, virtual_col, width));
        if virtual_col > target {
            return index;
        }
    }
    let extra = usize::try_from(target.saturating_sub(virtual_col)).unwrap_or(0);
    (maximal + extra).min(chars.len())
}

fn cell_width(ch: char, virtual_col: u32, width: u32) -> u32 {
    if ch == '\t' {
        width - virtual_col % width
    } else {
        1
    }
}

fn line_edge(buf: &TextBuffer, head: Offset, end: bool) -> Offset {
    let place = buf.line_col(head);
    let col = if end { u32::MAX } else { 0 };
    let raw = buf.offset(LineCol {
        line: place.line,
        col,
    });
    if end { snap_floor(buf, raw) } else { raw }
}

fn smart_home(buf: &TextBuffer, head: Offset) -> Offset {
    let place = buf.line_col(head);
    let text = line_str(buf, place.line);
    let col = smart_home_col(&text, col_index(place.col));
    buf.offset(LineCol {
        line: place.line,
        col: u32::try_from(col).unwrap_or(u32::MAX),
    })
}

fn smart_home_col(line: &str, col: usize) -> usize {
    match line.chars().position(|ch| !ch.is_whitespace()) {
        Some(first) if col != first => first,
        _ => 0,
    }
}

/// Return the previous grapheme boundary. The offset stays inside the buffer.
///
/// The read is a short slice around the cursor. It does not copy the whole line.
pub(crate) fn grapheme_before(buf: &TextBuffer, offset: Offset) -> Offset {
    let len = buf.len().0;
    let at = offset.0.min(len);
    if at == 0 {
        return Offset(0);
    }
    let mut radius = 8usize;
    loop {
        let start = at.saturating_sub(radius);
        let rel = at - start;
        let text = copy_text(buf, start, at);
        let prev = prev_bound(&text, rel);
        if prev > 0 || start == 0 || radius >= 256 {
            return Offset(start + prev);
        }
        radius = radius.saturating_mul(2);
    }
}

/// Return the next grapheme boundary. The offset stays inside the buffer.
///
/// The read is a short slice around the cursor. It does not copy the whole line.
pub(crate) fn grapheme_after(buf: &TextBuffer, offset: Offset) -> Offset {
    let len = buf.len().0;
    let at = offset.0.min(len);
    if at == len {
        return Offset(len);
    }
    let mut radius = 8usize;
    loop {
        let end = at.saturating_add(radius).min(len);
        let text = copy_text(buf, at, end);
        let next = next_bound(&text, 0);
        let abs = at + next;
        if abs < end || end == len || radius >= 256 {
            return Offset(abs.min(len));
        }
        radius = radius.saturating_mul(2);
    }
}

/// Move an offset that sits inside a cluster back to the cluster start.
fn snap_floor(buf: &TextBuffer, offset: Offset) -> Offset {
    let at = clamp_offset(buf, offset);
    if at.0 == 0 {
        return at;
    }
    let prev = grapheme_before(buf, at);
    if grapheme_after(buf, prev).0 == at.0 {
        at
    } else {
        prev
    }
}

fn next_bound(text: &str, rel: usize) -> usize {
    let mut start = 0usize;
    for cluster in text.graphemes(true) {
        let next = start + cluster.chars().count();
        if start >= rel || next > rel {
            return next;
        }
        start = next;
    }
    start
}

fn prev_bound(text: &str, rel: usize) -> usize {
    let mut start = 0usize;
    for cluster in text.graphemes(true) {
        let next = start + cluster.chars().count();
        if next >= rel {
            return start;
        }
        start = next;
    }
    start
}

fn snap(buf: &TextBuffer, offset: Offset, forward: bool) -> Offset {
    let at = clamp_offset(buf, offset);
    let prev = grapheme_before(buf, at);
    if grapheme_after(buf, prev).0 == at.0 {
        return at;
    }
    if forward {
        grapheme_after(buf, at)
    } else {
        prev
    }
}

fn line_prefix(buf: &TextBuffer, line: u32, chars: u32) -> String {
    let start = buf.offset(LineCol { line, col: 0 }).0;
    let end = buf
        .offset(LineCol {
            line,
            col: u32::MAX,
        })
        .0;
    let take = usize::try_from(chars).unwrap_or(usize::MAX);
    copy_text(buf, start, start.saturating_add(take).min(end))
}

fn clamp_offset(buf: &TextBuffer, offset: Offset) -> Offset {
    Offset(offset.0.min(buf.len().0))
}

fn copy_text(buf: &TextBuffer, start: usize, end: usize) -> String {
    let len = buf.len().0;
    let start = start.min(len);
    let end = end.min(len);
    if start >= end {
        return String::new();
    }
    buf.slice(CharRange::new(Offset(start), Offset(end)))
        .to_string()
}

fn line_str(buf: &TextBuffer, line: u32) -> String {
    let start = buf.offset(LineCol { line, col: 0 }).0;
    let end = buf
        .offset(LineCol {
            line,
            col: u32::MAX,
        })
        .0;
    copy_text(buf, start, end)
}

fn chars_of(buf: &TextBuffer, line: u32) -> Vec<char> {
    line_str(buf, line).chars().collect()
}

#[derive(Copy, Clone)]
struct Pos {
    line: u32,
    col: u32,
}

fn pos_of(buf: &TextBuffer, offset: Offset) -> Pos {
    let place = buf.line_col(clamp_offset(buf, offset));
    Pos {
        line: place.line,
        col: place.col,
    }
}

fn offset_of(buf: &TextBuffer, pos: Pos) -> Offset {
    buf.offset(LineCol {
        line: pos.line,
        col: pos.col,
    })
}

fn col_index(col: u32) -> usize {
    usize::try_from(col).unwrap_or(usize::MAX)
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Kind {
    Space,
    Word,
    Other,
}

fn kind_of(ch: char) -> Kind {
    if ch.is_whitespace() {
        Kind::Space
    } else if is_in_word(ch) {
        Kind::Word
    } else {
        Kind::Other
    }
}

fn is_in_word(ch: char) -> bool {
    !is_default_delimiter(ch) && !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '`')
}

fn is_default_delimiter(ch: char) -> bool {
    let Ok(byte) = u8::try_from(ch) else {
        return false;
    };
    b"\t !%&()*+,-./:;<=>?[\\]^{|}~".contains(&byte)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_next(buf: &TextBuffer, offset: Offset) -> Offset {
    let pos = pos_of(buf, offset);
    let chars = chars_of(buf, pos.line);
    let landed = if col_index(pos.col) >= chars.len() {
        let wrapped = wrap_right(buf, pos);
        let wrapped_chars = chars_of(buf, wrapped.line);
        skip_spaces_forward(&wrapped_chars, wrapped)
    } else if chars
        .get(col_index(pos.col))
        .copied()
        .is_some_and(|ch| kind_of(ch) == Kind::Word)
    {
        cursor_forward(&chars, pos)
    } else {
        let skipped = skip_other_forward(&chars, pos);
        skip_spaces_forward(&chars, skipped)
    };
    snap(buf, offset_of(buf, landed), true)
}

fn word_prev(buf: &TextBuffer, offset: Offset) -> Offset {
    let pos = pos_of(buf, offset);
    let landed = if pos.col == 0 {
        wrap_left(buf, pos)
    } else {
        let chars = chars_of(buf, pos.line);
        let skipped = skip_spaces_back(&chars, pos);
        land_back(buf, &chars, skipped)
    };
    snap(buf, offset_of(buf, landed), false)
}

fn land_back(buf: &TextBuffer, chars: &[char], pos: Pos) -> Pos {
    if pos.col == 0 {
        return wrap_left(buf, pos);
    }
    match prev_kind(chars, pos.col) {
        Some(Kind::Word) => cursor_back(chars, pos),
        Some(Kind::Other) => skip_other_back(chars, pos),
        Some(Kind::Space) | None => pos,
    }
}

fn wrap_right(buf: &TextBuffer, pos: Pos) -> Pos {
    let last = buf.line_count().saturating_sub(1);
    if pos.line < last {
        Pos {
            line: pos.line + 1,
            col: 0,
        }
    } else {
        pos
    }
}

fn wrap_left(buf: &TextBuffer, pos: Pos) -> Pos {
    if pos.line == 0 {
        return Pos { line: 0, col: 0 };
    }
    let line = pos.line - 1;
    let col = u32::try_from(chars_of(buf, line).len()).unwrap_or(u32::MAX);
    Pos { line, col }
}

fn skip_spaces_forward(chars: &[char], mut pos: Pos) -> Pos {
    while chars
        .get(col_index(pos.col))
        .copied()
        .is_some_and(|ch| ch.is_whitespace())
    {
        pos.col = pos.col.saturating_add(1);
    }
    pos
}

fn skip_other_forward(chars: &[char], mut pos: Pos) -> Pos {
    while let Some(ch) = chars.get(col_index(pos.col)).copied() {
        if kind_of(ch) != Kind::Other {
            break;
        }
        pos.col = pos.col.saturating_add(1);
    }
    pos
}

fn skip_spaces_back(chars: &[char], mut pos: Pos) -> Pos {
    while pos.col > 0 && prev_kind(chars, pos.col) == Some(Kind::Space) {
        pos.col -= 1;
    }
    pos
}

fn skip_other_back(chars: &[char], mut pos: Pos) -> Pos {
    while pos.col > 0 && prev_kind(chars, pos.col) == Some(Kind::Other) {
        pos.col -= 1;
    }
    pos
}

fn prev_kind(chars: &[char], col: u32) -> Option<Kind> {
    let index = col_index(col).checked_sub(1)?;
    chars.get(index).copied().map(kind_of)
}

fn cursor_forward(chars: &[char], pos: Pos) -> Pos {
    let start = col_index(pos.col).min(chars.len());
    let mut col = start;
    while col < chars.len() && is_word_char(chars[col]) {
        col += 1;
    }
    while col < chars.len() && chars[col].is_whitespace() {
        col += 1;
    }
    if col == start {
        col = start.saturating_add(1).min(chars.len());
    }
    Pos {
        line: pos.line,
        col: u32::try_from(col).unwrap_or(u32::MAX),
    }
}

fn cursor_back(chars: &[char], pos: Pos) -> Pos {
    let Some(size) = isize::try_from(chars.len()).ok() else {
        return Pos {
            line: pos.line,
            col: 0,
        };
    };
    if size == 0 {
        return Pos {
            line: pos.line,
            col: 0,
        };
    }
    let column = isize::try_from(pos.col).unwrap_or(size);
    let mut col = column.min(size - 1) - 1;
    col = adjust_end_letter(chars, column, size, col);
    col = skip_back_spaces(chars, col);
    col = skip_back_word(chars, col);
    if col >= 0 && char_at(chars, col).is_some_and(|ch| !is_word_char(ch)) {
        col += 1;
    }
    Pos {
        line: pos.line,
        col: u32::try_from(jump_back(col, column)).unwrap_or(0),
    }
}

fn adjust_end_letter(chars: &[char], column: isize, size: isize, col: isize) -> isize {
    if column != size {
        return col;
    }
    let last_is_letter = char_at(chars, col + 1).is_some_and(|ch| ch.is_alphabetic());
    let before_is_space = col >= 0 && char_at(chars, col).is_some_and(|ch| ch.is_whitespace());
    if last_is_letter && before_is_space {
        col + 1
    } else {
        col
    }
}

fn skip_back_spaces(chars: &[char], mut col: isize) -> isize {
    while col > 0 && char_at(chars, col).is_some_and(|ch| ch.is_whitespace()) {
        col -= 1;
    }
    col
}

fn skip_back_word(chars: &[char], mut col: isize) -> isize {
    while col > 0 && char_at(chars, col).is_some_and(is_word_char) {
        col -= 1;
    }
    col
}

fn jump_back(col: isize, column: isize) -> isize {
    if col < 0 {
        0
    } else if col == column && column > 0 {
        column - 1
    } else {
        col
    }
}

fn char_at(chars: &[char], index: isize) -> Option<char> {
    let index = usize::try_from(index).ok()?;
    chars.get(index).copied()
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
    fn vertical_move_does_not_land_inside_a_grapheme() {
        let buf = TextBuffer::from_str("abc\ne\u{301}x");
        let moved = move_selection(
            &buf,
            Selection::caret(Offset(1)),
            Movement::Down,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(4));
        assert_eq!(moved.goal_col, Some(1));
        assert_ne!(moved.head, Offset(5));
    }

    #[test]
    fn line_end_and_vertical_move_do_not_split_crlf() {
        let buf = TextBuffer::from_str("a\r\nb");
        let end = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::LineEnd,
            Extend::No,
            &ctx(),
        );
        assert_eq!(end.head, Offset(1));
        assert_ne!(end.head, Offset(2));

        let buf = TextBuffer::from_str("ab\n\r\nc");
        let down = move_selection(
            &buf,
            Selection::caret(Offset(1)),
            Movement::Down,
            Extend::No,
            &ctx(),
        );
        assert_eq!(down.head, Offset(3));
        assert_ne!(down.head, Offset(4));
    }

    #[test]
    fn word_right_crosses_non_ascii_letters() {
        let buf = TextBuffer::from_str("héllo world");
        let moved = move_selection(
            &buf,
            Selection::caret(Offset(0)),
            Movement::WordRight,
            Extend::No,
            &ctx(),
        );
        assert_eq!(moved.head, Offset(6));
    }

    #[test]
    fn vertical_move_expands_tabs_using_tab_width() {
        let buf = TextBuffer::from_str("0123456789\n\t\tXXXX");
        let start = Selection::caret(Offset(4));
        let narrow = move_selection(
            &buf,
            start,
            Movement::Down,
            Extend::No,
            &MoveContext { tab_width: 4 },
        );
        let wide = move_selection(
            &buf,
            start,
            Movement::Down,
            Extend::No,
            &MoveContext { tab_width: 8 },
        );
        assert_eq!(narrow.head, Offset(12));
        assert_eq!(wide.head, Offset(11));
        assert_ne!(narrow.head, wide.head);
        assert_eq!(narrow.goal_col, Some(4));
        assert_eq!(wide.goal_col, Some(4));
    }

    #[test]
    fn right_on_a_long_line_stays_fast() {
        const COUNT: usize = 20_000;
        let text = "a".repeat(COUNT * 2);
        let buf = TextBuffer::from_str(&text);
        let ranges: Vec<Selection> = (0..COUNT)
            .map(|index| Selection::caret(Offset(index)))
            .collect();
        let mut sel = Selections::new(ranges, 0);
        let started = std::time::Instant::now();
        move_all(&buf, &mut sel, Movement::Right, Extend::No, &ctx());
        let elapsed = started.elapsed();
        assert_eq!(sel.len(), COUNT);
        assert_eq!(sel.primary().head, Offset(1));
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "right on {COUNT} carets took {elapsed:?}"
        );
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
