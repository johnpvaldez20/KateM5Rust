// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Bracket pairs for a one-character insert.
//!
//! `plan_pair` returns a plain insert, a wrap, or a type-over.
//! Pairing is always on.

use kate_text::Offset;

/// The effect of one typed string on one selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairPlan {
    /// Insert the typed text.
    Plain,
    /// Wrap the span in an opener and a closer.
    ///
    /// The caret stays after the opener when `caret_after_open` is true.
    /// The caret stays after the closer when `caret_after_open` is false.
    Wrap { caret_after_open: bool },
    /// Leave the text unchanged and move the caret one scalar right.
    TypeOver,
}

/// Return the pair plan for `typed` at `selection`.
///
/// More than one scalar is plain.
/// A caret steps over a closer when the next scalar is that closer.
/// A quote after an ASCII letter, digit, or underscore stays plain.
/// That quote rule does not apply to a type-over.
/// Any other opener wraps the span.
pub fn plan_pair(
    buf: &kate_text::TextBuffer,
    selection: crate::Selection,
    typed: &str,
) -> PairPlan {
    let Some(ch) = one_scalar(typed) else {
        return PairPlan::Plain;
    };
    if is_type_over(buf, selection, ch) {
        return PairPlan::TypeOver;
    }
    if quote_stays_plain(buf, selection, ch) {
        return PairPlan::Plain;
    }
    if is_opener(ch) {
        return PairPlan::Wrap {
            caret_after_open: selection.is_caret(),
        };
    }
    PairPlan::Plain
}

/// Return true when `typed` cannot wrap or step over.
pub(super) fn cannot_pair(typed: &str) -> bool {
    !one_scalar(typed).is_some_and(|ch| is_opener(ch) || is_closer(ch))
}

/// Return `opener`, `old`, and the matching closer.
pub(super) fn wrapped_text(opener: char, old: &str) -> String {
    let closer = closer_for(opener);
    format!("{opener}{old}{closer}")
}

fn closer_for(opener: char) -> char {
    match opener {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        other => other,
    }
}

fn one_scalar(typed: &str) -> Option<char> {
    let mut chars = typed.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(ch)
}

fn is_type_over(buf: &kate_text::TextBuffer, selection: crate::Selection, ch: char) -> bool {
    selection.is_caret() && is_closer(ch) && char_at(buf, selection.head.0) == Some(ch)
}

fn quote_stays_plain(buf: &kate_text::TextBuffer, selection: crate::Selection, ch: char) -> bool {
    is_quote(ch) && char_before(buf, selection.min()).is_some_and(is_ascii_word)
}

fn char_before(buf: &kate_text::TextBuffer, at: Offset) -> Option<char> {
    let index = at.0.checked_sub(1)?;
    char_at(buf, index)
}

fn char_at(buf: &kate_text::TextBuffer, index: usize) -> Option<char> {
    if index >= buf.len().0 {
        None
    } else {
        Some(buf.text().char(index))
    }
}

fn is_opener(ch: char) -> bool {
    matches!(ch, '(' | '[' | '{' | '"' | '\'')
}

fn is_closer(ch: char) -> bool {
    matches!(ch, ')' | ']' | '}' | '"' | '\'')
}

fn is_quote(ch: char) -> bool {
    matches!(ch, '"' | '\'')
}

fn is_ascii_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::super::insert_text;
    use crate::selection::{Selection, Selections};
    use kate_text::{Offset, TextBuffer};

    fn type_at(text: &str, sel: &Selections, typed: &str) -> (String, Selections) {
        let mut buf = TextBuffer::from_str(text);
        let (tx, after) = insert_text(&buf, sel, typed).expect("the insert is valid");
        buf.apply(&tx).expect("the insert matches the buffer");
        (buf.text().to_string(), after)
    }

    #[test]
    fn caret_gets_a_pair_and_sits_inside() {
        let sel = Selections::single(Selection::caret(Offset(1)));
        let (text, after) = type_at("ab", &sel, "(");
        assert_eq!(text, "a()b");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(2));

        let sel = Selections::single(Selection {
            anchor: Offset(1),
            head: Offset(2),
            goal_col: None,
        });
        let (text, after) = type_at("ab", &sel, "(");
        assert_eq!(text, "a(b)");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(4));
    }

    #[test]
    fn type_over_and_quote_after_a_letter() {
        let sel = Selections::single(Selection::caret(Offset(2)));
        let (text, after) = type_at("a()", &sel, ")");
        assert_eq!(text, "a()");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(3));

        let sel = Selections::single(Selection::caret(Offset(3)));
        let (text, after) = type_at("don", &sel, "'");
        assert_eq!(text, "don'");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(4));
    }

    #[test]
    fn two_carets_each_get_a_pair() {
        let sel = Selections::new(
            vec![Selection::caret(Offset(0)), Selection::caret(Offset(3))],
            0,
        );
        let (text, after) = type_at("ab cd", &sel, "(");
        assert_eq!(text, "()ab ()cd");
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|selection| selection.is_caret()));
        let heads: Vec<Offset> = after.iter().map(|selection| selection.head).collect();
        assert_eq!(heads, vec![Offset(1), Offset(6)]);
    }

    #[test]
    fn each_opener_wraps_and_a_closer_steps_over() {
        let pairs = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')];
        for (open, close) in pairs {
            let sel = Selections::single(Selection::caret(Offset(0)));
            let (text, after) = type_at("z", &sel, &open.to_string());
            assert_eq!(text, format!("{open}{close}z"));
            assert_eq!(after.primary().head, Offset(1));

            let existing = format!("a{close}");
            let sel = Selections::single(Selection::caret(Offset(1)));
            let (text, after) = type_at(&existing, &sel, &close.to_string());
            assert_eq!(text, existing);
            assert_eq!(after.primary().head, Offset(2));
        }

        let sel = Selections::single(Selection::caret(Offset(2)));
        let (text, after) = type_at("a_", &sel, "'");
        assert_eq!(text, "a_'");
        assert_eq!(after.primary().head, Offset(3));

        let (text, after) = type_at("a9", &sel, "\"");
        assert_eq!(text, "a9\"");
        assert_eq!(after.primary().head, Offset(3));

        let (text, after) = type_at("a ", &sel, "\"");
        assert_eq!(text, "a \"\"");
        assert_eq!(after.primary().head, Offset(3));
    }

    #[test]
    fn a_backward_range_wraps_and_ends_after_the_closer() {
        let sel = Selections::single(Selection {
            anchor: Offset(2),
            head: Offset(1),
            goal_col: None,
        });
        let (text, after) = type_at("ab", &sel, "[");
        assert_eq!(text, "a[b]");
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().head, Offset(4));
    }

    #[test]
    fn one_caret_steps_over_while_another_inserts() {
        let sel = Selections::new(
            vec![Selection::caret(Offset(1)), Selection::caret(Offset(4))],
            1,
        );
        let (text, after) = type_at("()ab", &sel, ")");
        assert_eq!(text, "()ab)");
        assert!(after.iter().all(|selection| selection.is_caret()));
        let heads: Vec<Offset> = after.iter().map(|selection| selection.head).collect();
        assert_eq!(heads, vec![Offset(2), Offset(5)]);
        assert_eq!(after.primary().head, Offset(5));

        let sel = Selections::new(
            vec![
                Selection {
                    anchor: Offset(1),
                    head: Offset(3),
                    goal_col: None,
                },
                Selection::caret(Offset(4)),
            ],
            0,
        );
        let (text, after) = type_at("abcd", &sel, "(");
        assert_eq!(text, "a(bc)d()");
        let heads: Vec<Offset> = after.iter().map(|selection| selection.head).collect();
        assert_eq!(heads, vec![Offset(5), Offset(7)]);
        assert_eq!(after.primary().head, Offset(5));
    }
}
