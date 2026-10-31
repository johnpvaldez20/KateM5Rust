// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Replace-all as one transaction.

use crate::escape::{self, slice_text};
use crate::regex_find;
use crate::{Pattern, SearchError, SearchQuery, find_all};
use kate_text::{Edit, TextBuffer, Transaction};

pub(crate) fn replace_all(
    buf: &TextBuffer,
    q: &SearchQuery,
    replacement: &str,
) -> Result<(Transaction, usize), SearchError> {
    let found = find_all(buf, q)?;
    let names = capture_names(q)?;
    let rope = buf.text();
    let literal = matches!(q.pattern, Pattern::Plain(_));
    let mut edits = Vec::with_capacity(found.len());
    for hit in &found {
        let new = if literal {
            replacement.to_string()
        } else {
            escape::expand(replacement, hit, rope, names.as_deref())
        };
        edits.push(Edit {
            start: hit.range.start(),
            old: slice_text(rope, hit.range),
            new,
        });
    }
    edits.sort_by_key(|edit| edit.start);
    let edits = keep_disjoint(edits);
    let count = edits.len();
    let tx = Transaction::new(edits).expect("filtered search edits do not overlap");
    Ok((tx, count))
}

fn capture_names(q: &SearchQuery) -> Result<Option<Vec<Option<String>>>, SearchError> {
    match &q.pattern {
        Pattern::Regex(pattern) => Ok(Some(regex_find::capture_name_list(
            pattern,
            q.case_sensitive,
        )?)),
        Pattern::Plain(_) | Pattern::EscapeSequences(_) => Ok(None),
    }
}

/// The edits are sorted by start. Each one can overlap only the last kept edit.
fn keep_disjoint(edits: Vec<Edit>) -> Vec<Edit> {
    let mut kept = Vec::new();
    for edit in edits {
        let overlaps_last = kept.last().is_some_and(|prev| edits_overlap(prev, &edit));
        if !overlaps_last {
            kept.push(edit);
        }
    }
    kept
}

fn edits_overlap(left: &Edit, right: &Edit) -> bool {
    left.start.0 < right.old_end().0 && right.start.0 < left.old_end().0
}
