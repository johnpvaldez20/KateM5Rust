// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Key names, caret marks, and the scroll jump.
//!
//! This module does not import GPUI.
//! `session` asks for a command. `paint` reads the marks.

use kate_core::{Command, Selections};
use kate_text::TextBuffer;

/// One caret, in UTF-8 bytes on its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretMark {
    pub line: u32,
    pub byte: usize,
}

/// One selection piece on one line.
///
/// The byte range is half-open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelSpan {
    pub line: u32,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// UTF-8 byte offset of char column `col` in one line.
///
/// The sum matches `paint::byte_at_char`.
pub fn line_byte(line: &str, col: u32) -> usize {
    let _ = (line, col);
    0
}

/// One mark per selection head.
///
/// `byte` is [`line_byte`] of that line.
pub fn caret_marks(_buf: &TextBuffer, _selections: &Selections) -> Vec<CaretMark> {
    Vec::new()
}

/// Half-open UTF-8 spans for non-caret selections.
///
/// A caret adds no span.
/// A span stays on one line.
/// The newline belongs to the line that ends there.
pub fn selection_spans(_buf: &TextBuffer, _selections: &Selections) -> Vec<SelSpan> {
    Vec::new()
}

/// Empty text returns `None`.
///
/// Any other text returns [`Command::InsertText`] of that text.
/// Newlines in the committed string stay in the command.
pub fn command_for_commit(_text: &str) -> Option<Command> {
    None
}

/// `None` when the binding is not in the table.
pub fn command_for_binding(_binding: &str) -> Option<Command> {
    None
}

/// Return the first visible line that keeps the caret inside the window.
///
/// `visible_rows` of 0 leaves `first_line` unchanged.
/// A caret above the window moves the window to that line.
/// A caret below the window uses `caret_line + 1 - visible_rows`.
/// Clamp the caret to the last line.
/// Clamp the result so the window stays in range.
pub fn reveal_line(
    _first_line: u32,
    _visible_rows: u32,
    _caret_line: u32,
    _line_count: u32,
) -> u32 {
    0
}

/// Modifier order is ctrl, then alt, then shift, then cmd, then the key.
pub fn normalize_binding(binding: &str) -> String {
    binding.to_string()
}

/// Pixel offset for a jumped first line.
///
/// A bad height returns 0.
pub fn scroll_y_for_line(_first_line: u32, _line_height: f32) -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::{
        CaretMark, SelSpan, caret_marks, command_for_binding, command_for_commit, line_byte,
        normalize_binding, reveal_line, scroll_y_for_line, selection_spans,
    };
    use kate_core::{Command, Extend, Movement, Selection, Selections};
    use kate_text::{Offset, TextBuffer};

    #[test]
    fn binding_table_matches_existing_commands() {
        let rows = [
            ("left", Command::Move(Movement::Left, Extend::No)),
            ("right", Command::Move(Movement::Right, Extend::No)),
            ("up", Command::Move(Movement::Up, Extend::No)),
            ("down", Command::Move(Movement::Down, Extend::No)),
            ("shift-left", Command::Move(Movement::Left, Extend::Yes)),
            ("shift-right", Command::Move(Movement::Right, Extend::Yes)),
            ("shift-up", Command::Move(Movement::Up, Extend::Yes)),
            ("shift-down", Command::Move(Movement::Down, Extend::Yes)),
            ("backspace", Command::DeleteBackward),
            ("delete", Command::DeleteForward),
            ("enter", Command::Newline),
            ("tab", Command::InsertText("\t".to_string())),
            ("home", Command::Move(Movement::SmartHome, Extend::No)),
            ("end", Command::Move(Movement::LineEnd, Extend::No)),
            (
                "shift-home",
                Command::Move(Movement::SmartHome, Extend::Yes),
            ),
            ("shift-end", Command::Move(Movement::LineEnd, Extend::Yes)),
            ("cmd-left", Command::Move(Movement::LineStart, Extend::No)),
            ("cmd-right", Command::Move(Movement::LineEnd, Extend::No)),
            ("cmd-up", Command::Move(Movement::DocStart, Extend::No)),
            ("cmd-down", Command::Move(Movement::DocEnd, Extend::No)),
            (
                "shift-cmd-left",
                Command::Move(Movement::LineStart, Extend::Yes),
            ),
            (
                "shift-cmd-right",
                Command::Move(Movement::LineEnd, Extend::Yes),
            ),
            (
                "shift-cmd-up",
                Command::Move(Movement::DocStart, Extend::Yes),
            ),
            (
                "shift-cmd-down",
                Command::Move(Movement::DocEnd, Extend::Yes),
            ),
            ("cmd-a", Command::SelectAll),
            ("cmd-z", Command::Undo),
            ("shift-cmd-z", Command::Redo),
            ("cmd-s", Command::Save),
            ("alt-shift-down", Command::AddCursorBelow),
            ("alt-shift-up", Command::AddCursorAbove),
            ("escape", Command::RemoveSecondaryCursors),
        ];
        for (binding, command) in rows {
            assert_eq!(
                command_for_binding(binding),
                Some(command),
                "{binding} must return its command"
            );
        }
        assert_eq!(command_for_binding("cmd-o"), None);
        assert_eq!(command_for_binding("cmd-n"), None);
        assert_eq!(command_for_binding("cmd-w"), None);
        assert_eq!(command_for_binding("cmd-shift-s"), None);
        assert_eq!(command_for_binding("not-a-binding"), None);
    }

    #[test]
    fn commit_inserts_text_and_empty_commit_does_not() {
        assert_eq!(
            command_for_commit("a\nb"),
            Some(Command::InsertText("a\nb".to_string()))
        );
        assert_eq!(command_for_commit(""), None);
    }

    #[test]
    fn spans_cover_each_line_of_a_selection() {
        let buf = TextBuffer::from_str("ab\ncd");
        let range = Selections::single(Selection {
            anchor: Offset(1),
            head: Offset(4),
            goal_col: None,
        });
        assert_eq!(
            selection_spans(&buf, &range),
            vec![
                SelSpan {
                    line: 0,
                    start_byte: 1,
                    end_byte: 3,
                },
                SelSpan {
                    line: 1,
                    start_byte: 0,
                    end_byte: 1,
                },
            ]
        );
        let caret = Selections::single(Selection::caret(Offset(0)));
        assert_eq!(
            caret_marks(&buf, &caret),
            vec![CaretMark { line: 0, byte: 0 }]
        );
        assert!(selection_spans(&buf, &caret).is_empty());
        let acute = TextBuffer::from_str("\u{00e9}");
        let mark = Selections::single(Selection::caret(Offset(1)));
        assert_eq!(line_byte("\u{00e9}", 1), 2);
        assert_eq!(
            caret_marks(&acute, &mark),
            vec![CaretMark { line: 0, byte: 2 }]
        );
        assert!(selection_spans(&acute, &mark).is_empty());
    }

    #[test]
    fn reveal_line_puts_the_caret_in_the_window() {
        assert_eq!(reveal_line(10, 5, 12, 100), 10);
        assert_eq!(reveal_line(10, 5, 3, 100), 3);
        assert_eq!(reveal_line(10, 5, 20, 100), 16);
        assert_eq!(reveal_line(10, 0, 0, 100), 10);
        assert_eq!(reveal_line(4, 5, 100, 10), 5);
    }

    #[test]
    fn modifier_order_is_ctrl_alt_shift_cmd() {
        assert_eq!(normalize_binding("cmd-shift-z"), "shift-cmd-z");
        assert_eq!(normalize_binding("shift-cmd-z"), "shift-cmd-z");
        assert_eq!(command_for_binding("cmd-shift-z"), Some(Command::Redo));
        assert_eq!(command_for_binding("shift-cmd-z"), Some(Command::Redo));
        assert_eq!(
            command_for_binding("shift-cmd-left"),
            Some(Command::Move(Movement::LineStart, Extend::Yes))
        );
        assert_eq!(
            command_for_binding("alt-shift-down"),
            Some(Command::AddCursorBelow)
        );
    }

    #[test]
    fn scroll_y_matches_the_jumped_line() {
        assert_eq!(scroll_y_for_line(4, 20.0), 80.0);
        assert_eq!(scroll_y_for_line(4, 0.0), 0.0);
        assert_eq!(scroll_y_for_line(4, f32::NAN), 0.0);
        assert_eq!(scroll_y_for_line(4, f32::INFINITY), 0.0);
    }
}
