// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Find the bracket that pairs with one scalar.
//!
//! The pairs are parentheses, square brackets, and curly brackets.
//! The scan does not know syntax.
//! A quote is not a bracket.

use crate::{Offset, TextBuffer};

/// One step of a bracket scan.
enum Hit {
    Continue,
    Found,
    Mismatch,
}

/// Opener and closer for each pair.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// Returns the offset of the bracket that pairs with the scalar at `at`.
///
/// When the scalar at `at` is not a bracket and `at` is greater than 0, the scan uses the scalar at `at - 1`.
/// An offset past the end uses the last scalar when that scalar is a bracket.
/// A mismatched nest returns `None`.
pub fn matching_bracket(buf: &TextBuffer, at: Offset) -> Option<Offset> {
    let (index, ch) = bracket_at(buf, at)?;
    let (mate, is_open) = pair_of(ch)?;
    scan(buf, index, mate, is_open)
}

/// Returns `(mate, is_open)` when `ch` is a bracket.
fn pair_of(ch: char) -> Option<(char, bool)> {
    for (open, close) in PAIRS {
        if ch == open {
            return Some((close, true));
        }
        if ch == close {
            return Some((open, false));
        }
    }
    None
}

/// Returns the bracket scalar the scan starts on.
fn bracket_at(buf: &TextBuffer, at: Offset) -> Option<(usize, char)> {
    let len = buf.len().0;
    let index = scalar_index(at.0, len)?;
    let ch = buf.text().char(index);
    if pair_of(ch).is_some() {
        return Some((index, ch));
    }
    fallback_bracket(buf, at.0, len)
}

/// Returns `at` when it is in range. An offset past the end returns the last scalar.
fn scalar_index(at: usize, len: usize) -> Option<usize> {
    if at < len {
        Some(at)
    } else {
        len.checked_sub(1)
    }
}

/// Uses the previous scalar when `at` is in range and greater than 0.
fn fallback_bracket(buf: &TextBuffer, at: usize, len: usize) -> Option<(usize, char)> {
    if at == 0 || at >= len {
        return None;
    }
    let index = at - 1;
    let ch = buf.text().char(index);
    pair_of(ch).map(|_| (index, ch))
}

/// Scans forward when `nest_on_open` is true. Scans backward when it is false.
///
/// Forward starts on the next scalar. Backward uses a reversed `chars_at` iterator.
/// The loop does not call `Rope::char`.
fn scan(buf: &TextBuffer, origin: usize, mate: char, nest_on_open: bool) -> Option<Offset> {
    let text = buf.text();
    let mut pending = Vec::new();
    let (chars, step) = if nest_on_open {
        let start = origin.checked_add(1)?;
        (text.chars_at(start), 1isize)
    } else {
        (text.chars_at(origin).reversed(), -1isize)
    };
    let mut index = origin;
    for ch in chars {
        index = index.checked_add_signed(step)?;
        match on_scan(ch, &mut pending, mate, nest_on_open) {
            Hit::Found => return Some(Offset(index)),
            Hit::Mismatch => return None,
            Hit::Continue => {}
        }
    }
    None
}

/// Updates `pending` for one scalar.
///
/// A nested bracket pushes its mate.
/// The mate at depth 0 is the match.
/// A different bracket at the current depth is a mismatch.
fn on_scan(ch: char, pending: &mut Vec<char>, mate: char, nest_on_open: bool) -> Hit {
    let Some((other, is_open)) = pair_of(ch) else {
        return Hit::Continue;
    };
    if is_open == nest_on_open {
        pending.push(other);
        return Hit::Continue;
    }
    if let Some(expected) = pending.pop() {
        return if expected == ch {
            Hit::Continue
        } else {
            Hit::Mismatch
        };
    }
    if ch == mate {
        Hit::Found
    } else {
        Hit::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use crate::{Offset, TextBuffer, matching_bracket};

    #[test]
    fn chars_at_scan_matches_the_inner_pair() {
        let buf = TextBuffer::from_str("(a(b)c)");
        assert_eq!(matching_bracket(&buf, Offset(0)), Some(Offset(6)));
        assert_eq!(matching_bracket(&buf, Offset(2)), Some(Offset(4)));
        assert_eq!(matching_bracket(&buf, Offset(3)), Some(Offset(4)));
        assert_eq!(matching_bracket(&buf, Offset(1)), Some(Offset(6)));
    }

    #[test]
    fn nested_pair_matches_the_inner_closer() {
        let buf = TextBuffer::from_str("(a(b)c)");
        assert_eq!(matching_bracket(&buf, Offset(0)), Some(Offset(6)));
        assert_eq!(matching_bracket(&buf, Offset(2)), Some(Offset(4)));
        assert_eq!(matching_bracket(&buf, Offset(6)), Some(Offset(0)));
        assert_eq!(matching_bracket(&buf, Offset(4)), Some(Offset(2)));
        // A caret on `a` is not a bracket. The scalar at offset 0 is the opener.
        assert_eq!(matching_bracket(&buf, Offset(1)), Some(Offset(6)));
        // A caret on `b` is not a bracket. The scalar at offset 2 is the opener.
        assert_eq!(matching_bracket(&buf, Offset(3)), Some(Offset(4)));
        // A caret on `c` uses the closer at offset 4.
        assert_eq!(matching_bracket(&buf, Offset(5)), Some(Offset(2)));
        // An offset past the end uses the last closer.
        assert_eq!(matching_bracket(&buf, Offset(7)), Some(Offset(0)));
        assert_eq!(matching_bracket(&buf, Offset(8)), Some(Offset(0)));

        let nested = TextBuffer::from_str("([{}])");
        assert_eq!(matching_bracket(&nested, Offset(0)), Some(Offset(5)));
        assert_eq!(matching_bracket(&nested, Offset(1)), Some(Offset(4)));
        assert_eq!(matching_bracket(&nested, Offset(2)), Some(Offset(3)));
        assert_eq!(matching_bracket(&nested, Offset(3)), Some(Offset(2)));
        assert_eq!(matching_bracket(&nested, Offset(4)), Some(Offset(1)));
        assert_eq!(matching_bracket(&nested, Offset(5)), Some(Offset(0)));

        let word = TextBuffer::from_str("(é)");
        assert_eq!(matching_bracket(&word, Offset(0)), Some(Offset(2)));
        assert_eq!(matching_bracket(&word, Offset(1)), Some(Offset(2)));
    }

    #[test]
    fn mismatch_returns_none() {
        let buf = TextBuffer::from_str("((]");
        assert_eq!(matching_bracket(&buf, Offset(0)), None);

        let plain = TextBuffer::from_str("abc");
        assert_eq!(matching_bracket(&plain, Offset(0)), None);
        assert_eq!(matching_bracket(&plain, Offset(1)), None);
        assert_eq!(matching_bracket(&plain, Offset(3)), None);

        let quotes = TextBuffer::from_str("\"'\"");
        assert_eq!(matching_bracket(&quotes, Offset(0)), None);
        assert_eq!(matching_bracket(&quotes, Offset(1)), None);
        assert_eq!(matching_bracket(&quotes, Offset(2)), None);

        // The scalar before `b` is not a bracket, so the scan does not walk further left.
        let gap = TextBuffer::from_str("(ab)");
        assert_eq!(matching_bracket(&gap, Offset(2)), None);

        let crossed = TextBuffer::from_str("([)]");
        assert_eq!(matching_bracket(&crossed, Offset(0)), None);
        assert_eq!(matching_bracket(&crossed, Offset(3)), None);

        // A different closer at the same depth is not a match.
        let same_depth = TextBuffer::from_str("(])");
        assert_eq!(matching_bracket(&same_depth, Offset(0)), None);

        assert_eq!(matching_bracket(&TextBuffer::from_str(""), Offset(0)), None);
    }
}
