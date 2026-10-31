// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-search`: plain-text and regular-expression search over a rope.
//!
//! Plain and whole-word search walk rope chunks.
//! Regex search runs on the rope through `regex-cursor`.
//! It does not copy the buffer into one `String`.
//! Replace-all builds one [`Transaction`](kate_text::Transaction).
//!
//! Specified in `docs/design/01-data-structures.md`, section "Search".
//! Arrives with packet PKT-0010.

mod escape;
mod literal;
mod regex_find;
mod replace;
mod search;

use kate_text::{CharRange, Offset, TextBuffer};

/// The text to find.
///
/// Kate uses three search modes. These three variants are those modes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Pattern {
    /// Find the text as it is written.
    Plain(String),
    /// Expand escapes, then run a plain search.
    ///
    /// The escapes are `\n`, `\t`, `\xNN`, `\uNNNN`, and `\\`.
    EscapeSequences(String),
    /// Find with the linear-time `regex` engine.
    Regex(String),
}

/// One search request.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SearchQuery {
    /// The pattern.
    pub pattern: Pattern,
    /// When false, letters match in either case.
    pub case_sensitive: bool,
    /// When true, a plain match needs a word boundary on both sides.
    ///
    /// This flag does nothing for [`Pattern::Regex`]. Write `\b` in the pattern.
    pub whole_words: bool,
    /// Search only this range. `None` means the whole buffer.
    pub range: Option<CharRange>,
}

/// One hit.
///
/// `captures` is empty for a plain or escape hit.
/// For a regex hit, `captures[0]` is the whole match.
/// A `None` entry is a group that did not take part in the hit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Match {
    /// The whole hit.
    pub range: CharRange,
    /// Capture ranges. Empty when the pattern is not a regex.
    pub captures: Vec<Option<CharRange>>,
}

/// Find every non-overlapping hit.
///
/// An empty plain pattern reports an empty hit on each scalar boundary.
/// That is the same set as `str::match_indices`.
///
/// # Errors
///
/// Returns [`SearchError::InvalidRegex`] when the regex does not compile.
/// Returns [`SearchError::InvalidEscape`] when an escape sequence is not valid.
pub fn find_all(buf: &TextBuffer, q: &SearchQuery) -> Result<Vec<Match>, SearchError> {
    let (start, end) = clamp_range(buf, q.range);
    let rope = buf.text();
    match &q.pattern {
        Pattern::Plain(pattern) => Ok(literal::find(
            rope,
            pattern,
            q.case_sensitive,
            q.whole_words,
            start,
            end,
        )),
        Pattern::EscapeSequences(pattern) => {
            let expanded = escape::expand_pattern(pattern)?;
            Ok(literal::find(
                rope,
                &expanded,
                q.case_sensitive,
                q.whole_words,
                start,
                end,
            ))
        }
        Pattern::Regex(pattern) => regex_find::find(rope, pattern, q.case_sensitive, start, end),
    }
}

/// Find the next hit after `from`.
///
/// A non-empty hit that starts at `from` is a result.
/// An empty hit that starts at `from` is not a result.
/// When `wrap` is true and no hit remains, the search continues from the other end of the range.
///
/// # Errors
///
/// Returns the same errors as [`find_all`].
pub fn find_next(
    buf: &TextBuffer,
    q: &SearchQuery,
    from: Offset,
    backwards: bool,
    wrap: bool,
) -> Result<Option<Match>, SearchError> {
    let found = find_all(buf, q)?;
    let from = Offset(from.0.min(buf.len().0));
    let hit = if backwards {
        pick_back(&found, from, wrap)
    } else {
        pick_forward(&found, from, wrap)
    };
    Ok(hit)
}

/// Build one transaction that replaces every hit.
///
/// The `usize` is the number of edits in the transaction.
/// The replacement text accepts `$0` to `$9`, `${name}`, `\0` to `\9`, `$$`, `\\`, `\n`, and `\t`.
/// `${name}` is resolved only for a regex pattern.
/// [`expand_replacement`] has no pattern, so a name there stays as text.
///
/// # Errors
///
/// Returns the same errors as [`find_all`].
pub fn replace_all(
    buf: &TextBuffer,
    q: &SearchQuery,
    replacement: &str,
) -> Result<(kate_text::Transaction, usize), SearchError> {
    replace::replace_all(buf, q, replacement)
}

/// Expand `$` groups, `\` groups, and the escapes `\n` and `\t`.
///
/// This function does not see capture names.
/// A `${name}` token is copied unchanged.
/// A missing `$` group expands to an empty string.
/// A missing `\` digit expands to that digit.
pub fn expand_replacement(replacement: &str, m: &Match, buf: &TextBuffer) -> String {
    escape::expand(replacement, m, buf.text(), None)
}

/// A search request was not valid.
#[derive(Debug, PartialEq, Eq)]
pub enum SearchError {
    /// The regex did not compile. The string is the engine message.
    InvalidRegex(String),
    /// An escape sequence is not valid. `at` is the byte index of the `\`.
    InvalidEscape { at: usize },
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::InvalidRegex(message) => write!(f, "invalid regex: {message}"),
            SearchError::InvalidEscape { at } => write!(f, "invalid escape at byte {at}"),
        }
    }
}

impl std::error::Error for SearchError {}

fn clamp_range(buf: &TextBuffer, range: Option<CharRange>) -> (usize, usize) {
    let len = buf.len().0;
    match range {
        None => (0, len),
        Some(span) => {
            let start = span.start().0.min(len);
            let end = span.end().0.min(len);
            (start, end)
        }
    }
}

fn empty_at(hit: &Match, from: Offset) -> bool {
    hit.range.is_empty() && hit.range.start() == from
}

fn pick_forward(found: &[Match], from: Offset, wrap: bool) -> Option<Match> {
    if let Some(hit) = found
        .iter()
        .find(|hit| hit.range.start() >= from && !empty_at(hit, from))
    {
        return Some(hit.clone());
    }
    if !wrap {
        return None;
    }
    found.iter().find(|hit| !empty_at(hit, from)).cloned()
}

fn pick_back(found: &[Match], from: Offset, wrap: bool) -> Option<Match> {
    if let Some(hit) = found
        .iter()
        .rev()
        .find(|hit| hit.range.end() <= from && !empty_at(hit, from))
    {
        return Some(hit.clone());
    }
    if !wrap {
        return None;
    }
    found.iter().rev().find(|hit| !empty_at(hit, from)).cloned()
}
