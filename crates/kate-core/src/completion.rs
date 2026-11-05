// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Words from the buffer that complete the prefix before the caret.

use std::collections::HashSet;

/// Return buffer words that start with the prefix before `at`.
///
/// A word is a run of Unicode letters, Unicode digits, an apostrophe, and `_`.
/// The prefix is the word characters immediately before `at`.
/// An empty prefix returns an empty list.
/// A limit of 0 returns an empty list.
/// The word that contains `at` is omitted.
/// Words after that word come first.
/// Words before that word come next.
/// Each word is returned once.
/// The match ignores letter case.
pub fn word_completions(
    buf: &kate_text::TextBuffer,
    at: kate_text::Offset,
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let len = buf.text().len_chars();
    let at = at.0.min(len);
    let start = rewind_word(buf, at);
    if start == at {
        return Vec::new();
    }
    let prefix = buf.text().slice(start..at).to_string();
    let after = forward_word(buf, at, len);
    let mut seen = HashSet::new();
    let mut found = matches_between(buf, after, len, &prefix, limit, &mut seen);
    if found.len() < limit {
        let room = limit - found.len();
        found.extend(matches_between(buf, 0, start, &prefix, room, &mut seen));
    }
    found
}

/// Return the suffix the first matching word would insert.
///
/// The call uses `word_completions` with a limit of 1.
/// The suffix is that match without the prefix before the caret.
/// An empty prefix returns `None`.
/// A match equal to the prefix returns `None`.
/// The tail is the word characters after the caret.
/// A tail equal to the suffix returns `None`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn completion_suffix(buf: &kate_text::TextBuffer, at: kate_text::Offset) -> Option<String> {
    let word = word_completions(buf, at, 1).into_iter().next()?;
    let len = buf.text().len_chars();
    let caret = at.0.min(len);
    let start = rewind_word(buf, caret);
    let prefix = buf.text().slice(start..caret).to_string();
    let prefix_len = prefix.chars().count();
    let suffix: String = word.chars().skip(prefix_len).collect();
    if suffix.is_empty() {
        return None;
    }
    let tail_end = forward_word(buf, caret, len);
    if buf.text().slice(caret..tail_end) == suffix.as_str() {
        return None;
    }
    Some(suffix)
}

fn is_word_char(ch: char) -> bool {
    ch == '\'' || ch == '\u{2019}' || ch.is_alphanumeric() || ch == '_'
}

fn rewind_word(buf: &kate_text::TextBuffer, at: usize) -> usize {
    let text = buf.text();
    let mut index = at;
    while index > 0 && is_word_char(text.char(index - 1)) {
        index -= 1;
    }
    index
}

fn forward_word(buf: &kate_text::TextBuffer, mut index: usize, len: usize) -> usize {
    let text = buf.text();
    while index < len && is_word_char(text.char(index)) {
        index += 1;
    }
    index
}

fn has_ascii_prefix(word: &str, prefix: &str) -> bool {
    let mut chars = word.chars();
    prefix.chars().all(|ch| {
        let Some(next) = chars.next() else {
            return false;
        };
        next.to_lowercase().eq(ch.to_lowercase())
    })
}

fn matches_between(
    buf: &kate_text::TextBuffer,
    mut index: usize,
    end: usize,
    prefix: &str,
    limit: usize,
    seen: &mut HashSet<String>,
) -> Vec<String> {
    let mut found = Vec::new();
    let len = buf.text().len_chars();
    while index < end && found.len() < limit {
        if !is_word_char(buf.text().char(index)) {
            index += 1;
            continue;
        }
        let word_at = index;
        index = forward_word(buf, index, len);
        let word = buf.text().slice(word_at..index).to_string();
        if has_ascii_prefix(&word, prefix) && seen.insert(word.clone()) {
            found.push(word);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use kate_text::{Offset, TextBuffer};

    use super::{completion_suffix, word_completions};

    #[test]
    fn prefix_lists_later_words_once() {
        let buf = TextBuffer::from_str("alpha alphabet alphabet");
        let got = word_completions(&buf, Offset(4), 8);
        assert_eq!(got, vec!["alphabet".to_string()]);
    }

    #[test]
    fn an_earlier_word_completes_when_nothing_follows() {
        let buf = TextBuffer::from_str("alphabet alpha");
        let got = word_completions(&buf, Offset(13), 8);
        assert_eq!(got, vec!["alphabet".to_string()]);
    }

    #[test]
    fn later_match_stays_ahead_of_an_earlier_match() {
        let buf = TextBuffer::from_str("banana bandana band");
        let got = word_completions(&buf, Offset(10), 8);
        assert_eq!(got, vec!["band".to_string(), "banana".to_string()]);
        let one = word_completions(&buf, Offset(10), 1);
        assert_eq!(one, vec!["band".to_string()]);
    }

    #[test]
    fn a_repeated_word_stays_once() {
        let buf = TextBuffer::from_str("apple apricot apple");
        let got = word_completions(&buf, Offset(8), 8);
        assert_eq!(got, vec!["apple".to_string()]);
    }

    #[test]
    fn the_match_ignores_ascii_case() {
        let buf = TextBuffer::from_str("Alphabet alpha");
        let got = word_completions(&buf, Offset(13), 8);
        assert_eq!(got, vec!["Alphabet".to_string()]);
    }

    #[test]
    fn completion_suffix_is_the_rest_of_the_word() {
        let buf = TextBuffer::from_str("alpha alphabet");
        assert_eq!(completion_suffix(&buf, Offset(4)), Some("abet".to_string()));
        assert_eq!(completion_suffix(&buf, Offset(0)), None);
        let same = TextBuffer::from_str("alpha alpha");
        assert_eq!(completion_suffix(&same, Offset(5)), None);
    }

    #[test]
    fn completion_suffix_skips_a_copied_tail() {
        let buf = TextBuffer::from_str("alphabet alphabet");
        assert_eq!(completion_suffix(&buf, Offset(4)), None);
    }

    #[test]
    fn an_empty_prefix_and_a_zero_limit_stay_empty() {
        let buf = TextBuffer::from_str("alphabet alpha");
        assert_eq!(word_completions(&buf, Offset(9), 8), Vec::<String>::new());
        assert_eq!(word_completions(&buf, Offset(13), 0), Vec::<String>::new());
        let only = TextBuffer::from_str("alpha");
        assert_eq!(word_completions(&only, Offset(4), 8), Vec::<String>::new());
    }

    #[test]
    fn an_apostrophe_stays_inside_the_word() {
        let buf = TextBuffer::from_str("don't don");
        let got = word_completions(&buf, Offset(9), 4);
        assert_eq!(got, vec!["don't".to_string()]);
        assert_eq!(completion_suffix(&buf, Offset(9)), Some("'t".to_string()));
    }

    #[test]
    fn a_capital_word_completes_a_lower_prefix() {
        let buf = TextBuffer::from_str("Don't don");
        let got = word_completions(&buf, Offset(9), 4);
        assert_eq!(got, vec!["Don't".to_string()]);
        assert_eq!(completion_suffix(&buf, Offset(9)), Some("'t".to_string()));
    }

    #[test]
    fn a_unicode_letter_stays_inside_the_word() {
        let buf = TextBuffer::from_str("café caf");
        let got = word_completions(&buf, Offset(8), 4);
        assert_eq!(got, vec!["café".to_string()]);
        assert_eq!(completion_suffix(&buf, Offset(8)), Some("é".to_string()));
    }

    #[test]
    fn a_curly_apostrophe_stays_inside_the_word() {
        let buf = TextBuffer::from_str("don\u{2019}t don");
        let got = word_completions(&buf, Offset(9), 4);
        assert_eq!(got, vec!["don\u{2019}t".to_string()]);
        assert_eq!(
            completion_suffix(&buf, Offset(9)),
            Some("\u{2019}t".to_string())
        );
    }

    #[test]
    fn a_capital_e_acute_completes_a_small_prefix() {
        let buf = TextBuffer::from_str("\u{00C9}clair \u{00E9}cl");
        let got = word_completions(&buf, Offset(10), 4);
        assert_eq!(got, vec!["\u{00C9}clair".to_string()]);
        assert_eq!(completion_suffix(&buf, Offset(10)), Some("air".to_string()));
    }
}
