// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-search`: plain-text and regular-expression search over a rope.
//!
//! Plain and whole-word search walk rope chunks.
//! Regex search runs on the rope through `regex-cursor`.
//! It does not copy the buffer into one `String`.
//! Replace-all builds one transaction.
//!
//! Specified in `docs/design/01-data-structures.md`, section "Search".
//! Arrives with packet PKT-0010.

mod search;

use kate_text::{CharRange, Offset, TextBuffer, Transaction};

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
/// # Errors
///
/// Returns [`SearchError::InvalidRegex`] when the regex does not compile.
/// Returns [`SearchError::InvalidEscape`] when an escape sequence is not valid.
pub fn find_all(buf: &TextBuffer, q: &SearchQuery) -> Result<Vec<Match>, SearchError> {
    let _ = (buf, q);
    Ok(Vec::new())
}

/// Find the next hit after `from`.
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
    let _ = (buf, q, from, backwards, wrap);
    Ok(None)
}

/// Build one transaction that replaces every hit.
///
/// # Errors
///
/// Returns the same errors as [`find_all`].
pub fn replace_all(
    buf: &TextBuffer,
    q: &SearchQuery,
    replacement: &str,
) -> Result<(Transaction, usize), SearchError> {
    let _ = (buf, q, replacement);
    Ok((
        Transaction::new(Vec::new()).expect("an empty edit list is valid"),
        0,
    ))
}

/// Expand `$` groups, `\` groups, and the escapes `\n` and `\t`.
pub fn expand_replacement(replacement: &str, m: &Match, buf: &TextBuffer) -> String {
    let _ = (m, buf);
    replacement.to_string()
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
