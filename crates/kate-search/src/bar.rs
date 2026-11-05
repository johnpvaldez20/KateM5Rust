// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Find bar query and one-hit replace.
//!
//! The bar builds a [`SearchQuery`](crate::SearchQuery).
//! It replaces one hit or every hit.
//! A plain replacement copies the text.
//! A regex or escape replacement expands the text.

use kate_text::{Edit, Offset, TextBuffer, Transaction};

use crate::escape::{self, slice_text};
use crate::regex_find;
use crate::{Match, Pattern, SearchError, SearchQuery};

/// Search mode for the find bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindMode {
    /// Find the text as it is written.
    Plain,
    /// Expand escapes, then run a plain search.
    EscapeSequences,
    /// Find with a regular expression.
    Regex,
}

/// Pattern, replacement, and search flags for one find bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindBar {
    /// The text to find.
    pub pattern: String,
    /// The text that replaces a hit.
    pub replacement: String,
    /// Plain, escape, or regex.
    pub mode: FindMode,
    /// When false, letters match in either case.
    pub case_sensitive: bool,
    /// When true, a plain match needs a word boundary on both sides.
    pub whole_words: bool,
    /// When true, [`FindBar::find_from`] walks toward the start of the buffer.
    pub backwards: bool,
    /// When true, [`FindBar::find_from`] continues from the other end.
    pub wrap: bool,
}

impl FindBar {
    /// Maps this bar onto a [`SearchQuery`](crate::SearchQuery).
    ///
    /// The range is `None`. That range is the whole buffer.
    pub fn query(&self) -> SearchQuery {
        let pattern = match self.mode {
            FindMode::Plain => Pattern::Plain(self.pattern.clone()),
            FindMode::EscapeSequences => Pattern::EscapeSequences(self.pattern.clone()),
            FindMode::Regex => Pattern::Regex(self.pattern.clone()),
        };
        SearchQuery {
            pattern,
            case_sensitive: self.case_sensitive,
            whole_words: self.whole_words,
            range: None,
        }
    }

    /// Finds the next hit from `from`.
    ///
    /// The walk uses [`FindBar::backwards`] and [`FindBar::wrap`].
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`crate::find_next`].
    pub fn find_from(&self, buf: &TextBuffer, from: Offset) -> Result<Option<Match>, SearchError> {
        crate::find_next(buf, &self.query(), from, self.backwards, self.wrap)
    }

    /// Builds one edit for `hit`.
    ///
    /// A plain pattern copies the replacement.
    /// A regex or escape pattern expands `$` and `\` in the replacement.
    /// A regex name uses the capture names from the pattern.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::InvalidRegex`] when the regex does not compile.
    pub fn replace_one(&self, buf: &TextBuffer, hit: &Match) -> Result<Transaction, SearchError> {
        let new = expanded_replacement(self, buf, hit)?;
        let old = slice_text(buf.text(), hit.range);
        if old == new {
            return Ok(Transaction::default());
        }
        let edit = Edit {
            start: hit.range.start(),
            old,
            new,
        };
        Ok(Transaction::single(edit))
    }

    /// Replaces every hit with one transaction.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`crate::replace_all`].
    pub fn replace_all(&self, buf: &TextBuffer) -> Result<(Transaction, usize), SearchError> {
        crate::replace_all(buf, &self.query(), &self.replacement)
    }

    /// Counts hits in `buf`. An empty pattern yields zero.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`crate::find_all`].
    pub fn hit_count(&self, buf: &kate_text::TextBuffer) -> Result<usize, SearchError> {
        if self.pattern.is_empty() {
            return Ok(0);
        }
        let hits = crate::find_all(buf, &self.query())?;
        Ok(hits.len())
    }
}

/// A plain pattern copies the replacement.
/// A regex or escape pattern calls `escape::expand` and passes the capture names.
fn expanded_replacement(
    bar: &FindBar,
    buf: &TextBuffer,
    hit: &Match,
) -> Result<String, SearchError> {
    let names = match bar.mode {
        FindMode::Plain => return Ok(bar.replacement.clone()),
        FindMode::EscapeSequences => None,
        FindMode::Regex => Some(regex_find::capture_name_list(
            &bar.pattern,
            bar.case_sensitive,
        )?),
    };
    Ok(escape::expand(
        &bar.replacement,
        hit,
        buf.text(),
        names.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{FindBar, FindMode};
    use crate::{Pattern, SearchError, expand_replacement};
    use kate_text::{Offset, TextBuffer};

    fn sample(pattern: &str, replacement: &str, mode: FindMode, backwards: bool) -> FindBar {
        FindBar {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            mode,
            case_sensitive: true,
            whole_words: false,
            backwards,
            wrap: false,
        }
    }

    #[test]
    fn plain_replacement_keeps_a_dollar() {
        let mut buf = TextBuffer::from_str("a");
        let mut bar = sample("a", "$1", FindMode::Plain, false);
        bar.case_sensitive = false;
        bar.whole_words = true;
        let query = bar.query();
        assert!(matches!(query.pattern, Pattern::Plain(ref text) if text == "a"));
        assert!(!query.case_sensitive);
        assert!(query.whole_words);
        assert!(query.range.is_none());

        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("plain search does not fail")
            .expect("a matches");
        let tx = bar
            .replace_one(&buf, &hit)
            .expect("plain replace does not fail");
        assert_eq!(tx.edits().len(), 1);
        assert_eq!(tx.edits()[0].start, Offset(0));
        assert_eq!(tx.edits()[0].old, "a");
        assert_eq!(tx.edits()[0].new.chars().collect::<Vec<_>>(), ['$', '1']);
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.text().to_string(), "$1");

        let again = TextBuffer::from_str("a");
        let (all, count) = bar
            .replace_all(&again)
            .expect("plain replace does not fail");
        assert_eq!(count, 1);
        assert_eq!(all.edits()[0].new, "$1");
    }

    #[test]
    fn replace_one_expands_a_regex_group() {
        let mut buf = TextBuffer::from_str("cat");
        let bar = sample("(c)at", "$1", FindMode::Regex, false);
        let query = bar.query();
        assert!(matches!(query.pattern, Pattern::Regex(ref text) if text == "(c)at"));
        assert!(query.case_sensitive);
        assert!(!query.whole_words);
        assert!(query.range.is_none());

        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("regex compiles")
            .expect("cat matches");
        let tx = bar.replace_one(&buf, &hit).expect("regex compiles");
        assert_eq!(tx.edits().len(), 1);
        assert_eq!(tx.edits()[0].old, "cat");
        assert_eq!(tx.edits()[0].new, "c");
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.text().to_string(), "c");

        let mut named = TextBuffer::from_str("cat");
        let named_bar = sample("(?<head>c)at", "${head}", FindMode::Regex, false);
        let named_hit = named_bar
            .find_from(&named, Offset(0))
            .expect("regex compiles")
            .expect("cat matches");
        assert_eq!(
            expand_replacement("${head}", &named_hit, &named),
            "${head}",
            "the public expander has no capture names"
        );
        let named_tx = named_bar
            .replace_one(&named, &named_hit)
            .expect("regex compiles");
        assert_eq!(named_tx.edits()[0].new, "c");
        named
            .apply(&named_tx)
            .expect("the transaction matches the buffer");
        assert_eq!(named.text().to_string(), "c");

        let (all, count) = bar
            .replace_all(&TextBuffer::from_str("cat"))
            .expect("regex compiles");
        assert_eq!(count, 1);
        assert_eq!(all.edits()[0].new, "c");

        let bad = sample("(", "", FindMode::Regex, false);
        assert!(matches!(
            bad.find_from(&buf, Offset(0)),
            Err(SearchError::InvalidRegex(_))
        ));
    }

    #[test]
    fn find_from_walks_backward_without_wrap() {
        let buf = TextBuffer::from_str("aba");
        let bar = sample("a", "", FindMode::Plain, true);
        let hit = bar
            .find_from(&buf, Offset(3))
            .expect("plain search does not fail")
            .expect("a match ends at offset 3");
        assert_eq!(hit.range.start(), Offset(2));
        assert_eq!(hit.range.end(), Offset(3));
        assert!(
            bar.find_from(&buf, Offset(0))
                .expect("plain search does not fail")
                .is_none(),
            "wrap is false, so a backward search at offset 0 returns None"
        );
    }

    #[test]
    fn wrap_finds_a_hit_on_the_other_side() {
        let buf = TextBuffer::from_str("aba");
        let mut bar = sample("a", "", FindMode::Plain, true);
        bar.wrap = true;
        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("plain search does not fail")
            .expect("wrap finds the match at offset 2");
        assert_eq!(hit.range.start(), Offset(2));
        assert_eq!(hit.range.end(), Offset(3));
    }

    #[test]
    fn escape_replacement_expands_and_keeps_an_unknown_name() {
        let mut buf = TextBuffer::from_str("a");
        let bar = sample("a", "\\n${head}", FindMode::EscapeSequences, false);
        let query = bar.query();
        assert!(matches!(
            query.pattern,
            Pattern::EscapeSequences(ref text) if text == "a"
        ));
        assert!(query.range.is_none());
        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("plain search does not fail")
            .expect("a matches");
        let tx = bar
            .replace_one(&buf, &hit)
            .expect("escape replace does not fail");
        assert_eq!(tx.edits()[0].new, "\n${head}");
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.text().to_string(), "\n${head}");
    }

    #[test]
    fn hit_count_skips_an_empty_pattern() {
        let buf = TextBuffer::from_str("aba");
        let empty = sample("", "", FindMode::Plain, false);
        assert_eq!(empty.hit_count(&buf).expect("no error"), 0);

        let bar = sample("a", "", FindMode::Plain, false);
        assert_eq!(bar.hit_count(&buf).expect("no error"), 2);

        let bad = sample("(", "", FindMode::Regex, false);
        assert!(bad.hit_count(&buf).is_err());
    }

    #[test]
    fn a_same_replacement_inserts_nothing() {
        let buf = TextBuffer::from_str("aba");
        let bar = sample("a", "a", FindMode::Plain, false);
        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("plain search does not fail")
            .expect("a matches");
        let tx = bar
            .replace_one(&buf, &hit)
            .expect("plain replace does not fail");
        assert!(tx.is_empty(), "replacing a with a yields no edit");

        let bar = sample("a", "b", FindMode::Plain, false);
        let hit = bar
            .find_from(&buf, Offset(0))
            .expect("plain search does not fail")
            .expect("a matches");
        let tx = bar
            .replace_one(&buf, &hit)
            .expect("plain replace does not fail");
        assert_eq!(tx.edits().len(), 1);
        assert_eq!(tx.edits()[0].new, "b");
    }
}
