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
/// Each match adds 16.
/// Index 0 adds 8 and does not also get the separator bonus.
/// A later scalar adds 8 when the previous scalar is not ASCII alphanumeric.
/// The next scalar after the previous match adds 8.
/// Each skipped scalar between matches subtracts 1.
/// Case fold uses `char::to_lowercase`.
pub fn fuzzy_match(pattern: &str, candidate: &str) -> Option<FuzzyHit> {
    if pattern.is_empty() {
        return Some(FuzzyHit {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars: Vec<char> = candidate.chars().collect();
    let mut positions = Vec::new();
    let mut score = 0;
    let mut start = 0;
    let mut previous_match = None;
    for pattern_char in pattern.chars() {
        let index = next_index(&chars, start, pattern_char)?;
        score += score_one(&chars, index, previous_match);
        positions.push(index);
        previous_match = Some(index);
        start = index + 1;
    }
    Some(FuzzyHit { score, positions })
}

fn next_index(chars: &[char], start: usize, pattern_char: char) -> Option<usize> {
    let tail = chars.get(start..)?;
    let offset = tail.iter().position(|ch| same_fold(*ch, pattern_char))?;
    Some(start + offset)
}

fn same_fold(candidate_char: char, pattern_char: char) -> bool {
    candidate_char
        .to_lowercase()
        .eq(pattern_char.to_lowercase())
}

fn score_one(chars: &[char], index: usize, previous_match: Option<usize>) -> i32 {
    16 + boundary_bonus(chars, index) + gap_adjust(previous_match, index)
}

fn boundary_bonus(chars: &[char], index: usize) -> i32 {
    match index.checked_sub(1).map(|at| chars[at]) {
        None => 8,
        Some(ch) if !ch.is_ascii_alphanumeric() => 8,
        Some(_) => 0,
    }
}

fn gap_adjust(previous_match: Option<usize>, index: usize) -> i32 {
    let Some(previous_match) = previous_match else {
        return 0;
    };
    let skipped = i32::try_from(index - previous_match - 1).expect("a scalar gap fits in i32");
    let consecutive = i32::from(index == previous_match + 1) * 8;
    consecutive - skipped
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
