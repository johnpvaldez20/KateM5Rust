// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Words from the buffer that complete the prefix before the caret.

/// Return words that complete the prefix before `at`.
///
/// This body returns an empty list so the regression test fails.
#[allow(unused_variables)]
pub fn word_completions(
    buf: &kate_text::TextBuffer,
    at: kate_text::Offset,
    limit: usize,
) -> Vec<String> {
    Vec::new()
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
