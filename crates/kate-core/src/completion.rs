// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Words from the buffer that complete the prefix before the caret.

use std::collections::HashSet;

/// Return buffer words that start with the prefix before `at`.
///
/// A word is a run of ASCII letters, digits, and `_`.
/// The prefix is the word characters immediately before `at`.
/// An empty prefix returns an empty list.
/// A limit of 0 returns an empty list.
/// The word that contains `at` is omitted.
/// Words after that word come first.
/// Words before that word come next.
/// Each word is returned once.
/// The match is case sensitive.
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

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
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
        if word.starts_with(prefix) && seen.insert(word.clone()) {
            found.push(word);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use kate_text::{Offset, TextBuffer};

    use super::word_completions;

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
    fn the_match_stays_case_sensitive() {
        let buf = TextBuffer::from_str("Alphabet alpha");
        let got = word_completions(&buf, Offset(13), 8);
        assert_eq!(got, Vec::<String>::new());
    }

    #[test]
    fn an_empty_prefix_and_a_zero_limit_stay_empty() {
        let buf = TextBuffer::from_str("alphabet alpha");
        assert_eq!(word_completions(&buf, Offset(9), 8), Vec::<String>::new());
        assert_eq!(word_completions(&buf, Offset(13), 0), Vec::<String>::new());
        let only = TextBuffer::from_str("alpha");
        assert_eq!(word_completions(&only, Offset(4), 8), Vec::<String>::new());
    }
}
