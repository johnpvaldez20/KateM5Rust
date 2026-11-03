// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Key names, caret marks, and the scroll jump.
//!
//! This module does not import GPUI.
//! `session` asks for a command. `paint` reads the marks.

use kate_core::{Command, Extend, Movement, Selections};
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
    line.chars().take(col as usize).map(char::len_utf8).sum()
}

/// One mark per selection head.
///
/// `byte` is `line_byte` of that line.
pub fn caret_marks(buf: &TextBuffer, selections: &Selections) -> Vec<CaretMark> {
    selections
        .iter()
        .map(|selection| {
            let at = buf.line_col(selection.head);
            let text = content_line(buf, at.line);
            CaretMark {
                line: at.line,
                byte: line_byte(&text, at.col),
            }
        })
        .collect()
}

/// Half-open UTF-8 spans for non-caret selections.
///
/// A caret adds no span.
/// A span stays on one line.
/// The newline belongs to the line that ends there.
pub fn selection_spans(buf: &TextBuffer, selections: &Selections) -> Vec<SelSpan> {
    let mut spans = Vec::new();
    for selection in selections.iter() {
        if selection.is_caret() {
            continue;
        }
        push_spans(buf, selection, &mut spans);
    }
    spans
}

/// Spans for each selection inside `visible`.
///
/// `visible` is a half-open line range.
/// A caret adds no span.
/// A span stays on one line.
/// The walk starts at the first visible line of the selection.
/// The walk stops at the end of the visible range.
/// A line outside that range adds no span.
/// The newline belongs to the line that ends there.
#[cfg_attr(not(test), allow(dead_code))]
pub fn visible_selection_spans(
    buf: &TextBuffer,
    selections: &Selections,
    visible: std::ops::Range<u32>,
) -> Vec<SelSpan> {
    let mut spans = Vec::new();
    if visible.is_empty() {
        return spans;
    }
    for selection in selections.iter() {
        if selection.is_caret() {
            continue;
        }
        push_visible_spans(buf, selection, &visible, &mut spans);
    }
    spans
}

/// Empty text returns `None`.
///
/// Any other text returns `Command::InsertText` of that text.
/// Newlines in the committed string stay in the command.
pub fn command_for_commit(text: &str) -> Option<Command> {
    if text.is_empty() {
        None
    } else {
        Some(Command::InsertText(text.to_string()))
    }
}

/// `None` when the binding is not in the table.
pub fn command_for_binding(binding: &str) -> Option<Command> {
    let name = normalize_binding(binding);
    move_binding(&name)
        .or_else(|| text_binding(&name))
        .or_else(|| edit_binding(&name))
}

/// Return the first visible line that keeps the caret inside the window.
///
/// `visible_rows` of 0 leaves `first_line` unchanged.
/// A caret above the window moves the window to that line.
/// A caret below the window uses `caret_line + 1 - visible_rows`.
/// Clamp the caret to the last line.
/// Clamp the result so the window stays in range.
pub fn reveal_line(first_line: u32, visible_rows: u32, caret_line: u32, line_count: u32) -> u32 {
    if visible_rows == 0 {
        return first_line;
    }
    let last_line = line_count.saturating_sub(1);
    let caret = caret_line.min(last_line);
    let next = if caret < first_line {
        caret
    } else if caret >= first_line.saturating_add(visible_rows) {
        caret.saturating_add(1).saturating_sub(visible_rows)
    } else {
        first_line
    };
    next.min(last_line)
}

/// Modifier order is ctrl, then alt, then shift, then cmd, then the key.
pub fn normalize_binding(binding: &str) -> String {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut cmd = false;
    let mut key = String::new();
    for part in binding.split('-') {
        if part.is_empty() {
            push_token(&mut key, "-");
            continue;
        }
        let token = part.to_ascii_lowercase();
        match token.as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            "cmd" | "super" | "win" | "platform" => cmd = true,
            _ => push_token(&mut key, &token),
        }
    }
    let mut out = String::new();
    append_mod(&mut out, ctrl, "ctrl");
    append_mod(&mut out, alt, "alt");
    append_mod(&mut out, shift, "shift");
    append_mod(&mut out, cmd, "cmd");
    if !key.is_empty() {
        push_token(&mut out, &key);
    }
    out
}

/// Pixel offset for a jumped first line.
///
/// A bad height returns 0.
pub fn scroll_y_for_line(first_line: u32, line_height: f32) -> f32 {
    if !line_height.is_finite() || line_height <= 0.0 {
        return 0.0;
    }
    first_line as f32 * line_height
}

struct MoveBinding {
    name: &'static str,
    movement: Movement,
    extend: Extend,
}

const MOVE_BINDINGS: &[MoveBinding] = &[
    MoveBinding {
        name: "left",
        movement: Movement::Left,
        extend: Extend::No,
    },
    MoveBinding {
        name: "right",
        movement: Movement::Right,
        extend: Extend::No,
    },
    MoveBinding {
        name: "up",
        movement: Movement::Up,
        extend: Extend::No,
    },
    MoveBinding {
        name: "down",
        movement: Movement::Down,
        extend: Extend::No,
    },
    MoveBinding {
        name: "shift-left",
        movement: Movement::Left,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-right",
        movement: Movement::Right,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-up",
        movement: Movement::Up,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-down",
        movement: Movement::Down,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "home",
        movement: Movement::SmartHome,
        extend: Extend::No,
    },
    MoveBinding {
        name: "end",
        movement: Movement::LineEnd,
        extend: Extend::No,
    },
    MoveBinding {
        name: "shift-home",
        movement: Movement::SmartHome,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-end",
        movement: Movement::LineEnd,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "cmd-left",
        movement: Movement::LineStart,
        extend: Extend::No,
    },
    MoveBinding {
        name: "cmd-right",
        movement: Movement::LineEnd,
        extend: Extend::No,
    },
    MoveBinding {
        name: "cmd-up",
        movement: Movement::DocStart,
        extend: Extend::No,
    },
    MoveBinding {
        name: "cmd-down",
        movement: Movement::DocEnd,
        extend: Extend::No,
    },
    MoveBinding {
        name: "shift-cmd-left",
        movement: Movement::LineStart,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-cmd-right",
        movement: Movement::LineEnd,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-cmd-up",
        movement: Movement::DocStart,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "shift-cmd-down",
        movement: Movement::DocEnd,
        extend: Extend::Yes,
    },
    MoveBinding {
        name: "ctrl-6",
        movement: Movement::ToMatch,
        extend: Extend::No,
    },
    MoveBinding {
        name: "ctrl-shift-6",
        movement: Movement::ToMatch,
        extend: Extend::Yes,
    },
];

fn move_binding(name: &str) -> Option<Command> {
    MOVE_BINDINGS
        .iter()
        .find(|row| row.name == name)
        .map(|row| Command::Move(row.movement, row.extend))
}

fn text_binding(name: &str) -> Option<Command> {
    match name {
        "backspace" => Some(Command::DeleteBackward),
        "delete" => Some(Command::DeleteForward),
        "enter" => Some(Command::Newline),
        "tab" => Some(Command::InsertText("\t".to_string())),
        _ => None,
    }
}

fn edit_binding(name: &str) -> Option<Command> {
    match name {
        "cmd-a" => Some(Command::SelectAll),
        "cmd-z" => Some(Command::Undo),
        "shift-cmd-z" => Some(Command::Redo),
        "cmd-s" => Some(Command::Save),
        "alt-shift-down" => Some(Command::AddCursorBelow),
        "alt-shift-up" => Some(Command::AddCursorAbove),
        "escape" => Some(Command::RemoveSecondaryCursors),
        "ctrl-space" => Some(Command::CompleteWord),
        "ctrl-]" => Some(Command::Indent),
        "ctrl-[" => Some(Command::Unindent),
        "f5" => Some(Command::Reload),
        _ => None,
    }
}

fn push_spans(buf: &TextBuffer, selection: &kate_core::Selection, spans: &mut Vec<SelSpan>) {
    let start = buf.line_col(selection.min());
    let end = buf.line_col(selection.max());
    let mut line = start.line;
    loop {
        let text = rope_line(buf, line);
        let start_col = if line == start.line { start.col } else { 0 };
        let end_col = if line == end.line {
            end.col
        } else {
            char_count(&text)
        };
        let start_byte = line_byte(&text, start_col);
        let end_byte = line_byte(&text, end_col);
        if start_byte < end_byte {
            spans.push(SelSpan {
                line,
                start_byte,
                end_byte,
            });
        }
        if line >= end.line {
            break;
        }
        line += 1;
    }
}

fn push_visible_spans(
    buf: &TextBuffer,
    selection: &kate_core::Selection,
    visible: &std::ops::Range<u32>,
    spans: &mut Vec<SelSpan>,
) {
    let start = buf.line_col(selection.min());
    let end = buf.line_col(selection.max());
    if end.line < visible.start || start.line >= visible.end {
        return;
    }
    let first = start.line.max(visible.start);
    let last = end.line.min(visible.end.saturating_sub(1));
    for line in first..=last {
        let text = rope_line(buf, line);
        let start_col = if line == start.line { start.col } else { 0 };
        let end_col = if line == end.line {
            end.col
        } else {
            char_count(&text)
        };
        let start_byte = line_byte(&text, start_col);
        let end_byte = line_byte(&text, end_col);
        if start_byte < end_byte {
            spans.push(SelSpan {
                line,
                start_byte,
                end_byte,
            });
        }
    }
}

fn rope_line(buf: &TextBuffer, line: u32) -> String {
    let rope = buf.text();
    let Some(index) = usize::try_from(line).ok() else {
        return String::new();
    };
    if index >= rope.len_lines() {
        return String::new();
    }
    rope.line(index).to_string()
}

fn content_line(buf: &TextBuffer, line: u32) -> String {
    let mut text = rope_line(buf, line);
    if text.ends_with('\n') {
        text.pop();
    }
    if text.ends_with('\r') {
        text.pop();
    }
    text
}

fn char_count(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

fn push_token(out: &mut String, token: &str) {
    if !out.is_empty() {
        out.push('-');
    }
    out.push_str(token);
}

fn append_mod(out: &mut String, on: bool, name: &str) {
    if on {
        push_token(out, name);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaretMark, SelSpan, caret_marks, command_for_binding, command_for_commit, line_byte,
        normalize_binding, reveal_line, scroll_y_for_line, selection_spans,
        visible_selection_spans,
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
    fn visible_spans_cover_only_the_window() {
        let text = "x\n".repeat(4000);
        let buf = TextBuffer::from_str(&text);
        let all = Selections::single(Selection {
            anchor: Offset(0),
            head: buf.len(),
            goal_col: None,
        });
        assert_eq!(
            visible_selection_spans(&buf, &all, 10..12),
            vec![
                SelSpan {
                    line: 10,
                    start_byte: 0,
                    end_byte: 2,
                },
                SelSpan {
                    line: 11,
                    start_byte: 0,
                    end_byte: 2,
                },
            ]
        );
        let outside = Selections::single(Selection {
            anchor: Offset(0),
            head: Offset(1),
            goal_col: None,
        });
        assert_eq!(visible_selection_spans(&buf, &outside, 10..12), vec![]);
        assert_eq!(visible_selection_spans(&buf, &all, 10..10), vec![]);
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

    #[test]
    fn ctrl_6_moves_to_the_match_and_ctrl_space_completes() {
        assert_eq!(
            command_for_binding("ctrl-6"),
            Some(Command::Move(Movement::ToMatch, Extend::No))
        );
        assert_eq!(
            command_for_binding("ctrl-shift-6"),
            Some(Command::Move(Movement::ToMatch, Extend::Yes))
        );
        assert_eq!(
            command_for_binding("shift-ctrl-6"),
            Some(Command::Move(Movement::ToMatch, Extend::Yes))
        );
        assert_eq!(
            command_for_binding("ctrl-space"),
            Some(Command::CompleteWord)
        );
    }

    #[test]
    fn ctrl_bracket_indents_and_f5_reloads() {
        assert_eq!(command_for_binding("ctrl-]"), Some(Command::Indent));
        assert_eq!(command_for_binding("ctrl-["), Some(Command::Unindent));
        assert_eq!(command_for_binding("f5"), Some(Command::Reload));
    }
}
