// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Words from the buffer that complete the prefix before the caret.

use std::collections::HashSet;

/// Return later buffer words that start with the prefix before `at`.
///
/// A word is a run of ASCII letters, digits, and `_`.
/// The prefix is the word characters immediately before `at`.
/// An empty prefix returns an empty list.
/// A limit of 0 returns an empty list.
/// The word that contains `at` is omitted.
/// Only words after that word are candidates.
/// Each word is returned once, in first-seen order.
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
    let scan_from = forward_word(buf, at, len);
    later_matches(buf, scan_from, len, &prefix, limit)
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

fn later_matches(
    buf: &kate_text::TextBuffer,
    mut index: usize,
    len: usize,
    prefix: &str,
    limit: usize,
) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    while index < len && found.len() < limit {
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
}
