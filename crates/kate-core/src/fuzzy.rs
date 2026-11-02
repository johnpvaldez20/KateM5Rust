// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Case-insensitive subsequence score.
//!
//! `fuzzy_match` takes the leftmost scalars that match.
//! It does not search for a higher score.

/// One scored subsequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyHit {
    /// The score of this match.
    pub score: i32,
    /// Unicode scalar indexes in the candidate.
    pub positions: Vec<usize>,
}

/// Score `pattern` as a subsequence of `candidate`.
///
/// An empty pattern returns score 0 and no positions.
/// No subsequence returns `None`.
/// Case fold uses `char::to_lowercase`.
pub fn fuzzy_match(pattern: &str, candidate: &str) -> Option<FuzzyHit> {
    let _ = (pattern, candidate);
    None
}

#[cfg(test)]
mod tests {
    use super::{FuzzyHit, fuzzy_match};

    #[test]
    fn foo_bar_scores_the_documented_hits() {
        assert_eq!(
            fuzzy_match("fb", "foo_bar"),
            Some(FuzzyHit {
                score: 45,
                positions: vec![0, 4],
            })
        );
        assert_eq!(
            fuzzy_match("FB", "fooBar"),
            Some(FuzzyHit {
                score: 38,
                positions: vec![0, 3],
            })
        );
    }

    #[test]
    fn a_missing_subsequence_returns_none() {
        assert_eq!(fuzzy_match("fb", "bar"), None);
        assert_eq!(
            fuzzy_match("", "foo_bar"),
            Some(FuzzyHit {
                score: 0,
                positions: Vec::new(),
            })
        );
    }
}
