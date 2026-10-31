// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Plain search over rope chunks.
//!
//! Case-sensitive search compares UTF-8 bytes across chunk edges.
//! Case-insensitive search follows `str::to_lowercase`, including final sigma.

use std::collections::VecDeque;
use std::sync::OnceLock;

use crate::Match;
use kate_text::{CharRange, Offset};
use ropey::Rope;

pub(crate) fn find(
    rope: &Rope,
    needle: &str,
    case_sensitive: bool,
    whole_words: bool,
    start: usize,
    end: usize,
) -> Vec<Match> {
    let mut spans = if needle.is_empty() {
        empty_spans(rope, case_sensitive, start, end)
    } else if case_sensitive {
        find_exact(rope, needle, start, end, whole_words)
    } else {
        find_folded(rope, needle, start, end, whole_words)
    };
    if needle.is_empty() && whole_words {
        spans.retain(|span| is_whole_word(rope, span.0, span.1));
    }
    spans.retain(|span| span.0 >= start && span.1 <= end);
    spans
        .into_iter()
        .map(|(start_at, end_at)| Match {
            range: CharRange::new(Offset(start_at), Offset(end_at)),
            captures: Vec::new(),
        })
        .collect()
}

fn empty_spans(rope: &Rope, case_sensitive: bool, start: usize, end: usize) -> Vec<(usize, usize)> {
    if case_sensitive {
        (start..=end).map(|index| (index, index)).collect()
    } else {
        empty_folded(rope, start, end)
    }
}

fn empty_folded(rope: &Rope, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut found: Vec<(usize, usize)> = FoldIter::new(rope, start, end)
        .map(|(_, origin)| (origin, origin))
        .collect();
    found.push((end, end));
    found
}

fn find_exact(
    rope: &Rope,
    needle: &str,
    start: usize,
    end: usize,
    whole_words: bool,
) -> Vec<(usize, usize)> {
    let bytes = needle.as_bytes();
    if bytes.is_empty() {
        return Vec::new();
    }
    let Ok(start_b) = rope.try_char_to_byte(start) else {
        return Vec::new();
    };
    let Ok(end_b) = rope.try_char_to_byte(end) else {
        return Vec::new();
    };
    if bytes.len() > end_b.saturating_sub(start_b) {
        return Vec::new();
    }
    let map = ChunkMap::from_chunks(rope.chunks());
    let mut found = Vec::new();
    let mut pos = start_b;
    let last = end_b - bytes.len();
    while pos <= last {
        let Some(next) = next_exact(rope, &map, bytes, pos, whole_words, &mut found) else {
            break;
        };
        if next <= pos {
            break;
        }
        pos = next;
    }
    found
}

/// A rejected whole-word hit restarts one scalar later. An accepted hit ends the span.
fn next_exact(
    rope: &Rope,
    map: &ChunkMap<'_>,
    bytes: &[u8],
    pos: usize,
    whole_words: bool,
    found: &mut Vec<(usize, usize)>,
) -> Option<usize> {
    if map.starts_with(pos, bytes)
        && let Some(span) = char_span(rope, pos, pos + bytes.len())
    {
        return advance_after_hit(rope, pos, bytes.len(), span, whole_words, found);
    }
    pos.checked_add(1)
}

fn advance_after_hit(
    rope: &Rope,
    pos: usize,
    byte_len: usize,
    span: (usize, usize),
    whole_words: bool,
    found: &mut Vec<(usize, usize)>,
) -> Option<usize> {
    if !whole_words || is_whole_word(rope, span.0, span.1) {
        found.push(span);
        return pos.checked_add(byte_len);
    }
    rope.try_char_to_byte(span.0 + 1).ok()
}

fn find_folded(
    rope: &Rope,
    needle: &str,
    start: usize,
    end: usize,
    whole_words: bool,
) -> Vec<(usize, usize)> {
    let folded: Vec<char> = needle.to_lowercase().chars().collect();
    if folded.is_empty() {
        return empty_folded(rope, start, end);
    }
    let width = folded.len();
    let mut window = VecDeque::with_capacity(width);
    let mut found = Vec::new();
    for (ch, origin) in FoldIter::new(rope, start, end) {
        window.push_back((ch, origin));
        if window.len() < width {
            continue;
        }
        if window.len() > width {
            window.pop_front();
        }
        if !same_chars(&window, &folded) {
            continue;
        }
        let start_at = window[0].1;
        let end_at = window[width - 1].1 + 1;
        if !whole_words || is_whole_word(rope, start_at, end_at) {
            found.push((start_at, end_at));
            window.clear();
            continue;
        }
        while window.front().is_some_and(|item| item.1 == start_at) {
            window.pop_front();
        }
    }
    found
}

fn same_chars(window: &VecDeque<(char, usize)>, needle: &[char]) -> bool {
    window.len() == needle.len()
        && window
            .iter()
            .zip(needle)
            .all(|(item, want)| item.0 == *want)
}

fn char_span(rope: &Rope, start_b: usize, end_b: usize) -> Option<(usize, usize)> {
    let start = rope.try_byte_to_char(start_b).ok()?;
    let end = rope.try_byte_to_char(end_b).ok()?;
    if start > end {
        None
    } else {
        Some((start, end))
    }
}

fn is_whole_word(rope: &Rope, start: usize, end: usize) -> bool {
    is_boundary(rope, start) && is_boundary(rope, end)
}

fn is_boundary(rope: &Rope, index: usize) -> bool {
    let len = rope.len_chars();
    let left = index > 0 && is_word_char(rope.char(index - 1));
    let right = index < len && is_word_char(rope.char(index));
    left != right
}

struct ChunkMap<'a> {
    chunks: Vec<&'a str>,
    starts: Vec<usize>,
}

impl<'a> ChunkMap<'a> {
    fn from_chunks(chunks: impl Iterator<Item = &'a str>) -> Self {
        let mut stored = Vec::new();
        let mut starts = Vec::new();
        let mut acc = 0usize;
        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            starts.push(acc);
            acc += chunk.len();
            stored.push(chunk);
        }
        Self {
            chunks: stored,
            starts,
        }
    }

    fn locate(&self, pos: usize) -> Option<(&'a str, usize)> {
        if self.starts.is_empty() {
            return None;
        }
        let index = self.starts.partition_point(|start| *start <= pos);
        if index == 0 {
            return None;
        }
        let index = index - 1;
        let chunk = self.chunks[index];
        let off = pos - self.starts[index];
        if off >= chunk.len() {
            None
        } else {
            Some((chunk, off))
        }
    }

    fn starts_with(&self, pos: usize, needle: &[u8]) -> bool {
        let mut rest = needle;
        let mut pos = pos;
        while !rest.is_empty() {
            let Some((chunk, off)) = self.locate(pos) else {
                return false;
            };
            let bytes = chunk.as_bytes();
            let take = (bytes.len() - off).min(rest.len());
            if bytes[off..off + take] != rest[..take] {
                return false;
            }
            rest = &rest[take..];
            pos += take;
        }
        true
    }
}

struct FoldIter<'a> {
    rope: &'a Rope,
    next: usize,
    end: usize,
    pending: Vec<char>,
    pending_at: usize,
    origin: usize,
}

impl<'a> FoldIter<'a> {
    fn new(rope: &'a Rope, start: usize, end: usize) -> Self {
        Self {
            rope,
            next: start,
            end,
            pending: Vec::new(),
            pending_at: 0,
            origin: start,
        }
    }

    fn fill(&mut self) -> bool {
        self.pending.clear();
        self.pending_at = 0;
        if self.next >= self.end {
            return false;
        }
        let origin = self.next;
        let ch = self.rope.char(origin);
        self.next += 1;
        self.origin = origin;
        if ch == 'Σ' {
            self.pending.push(sigma_lower(self.rope, origin));
        } else {
            self.pending.extend(ch.to_lowercase());
        }
        !self.pending.is_empty()
    }
}

impl Iterator for FoldIter<'_> {
    type Item = (char, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pending_at >= self.pending.len() && !self.fill() {
            return None;
        }
        let ch = self.pending[self.pending_at];
        let origin = self.origin;
        self.pending_at += 1;
        Some((ch, origin))
    }
}

fn sigma_lower(rope: &Rope, index: usize) -> char {
    let len = rope.len_chars();
    let before = (0..index).rev().map(|at| rope.char(at));
    let after = (index + 1..len).map(|at| rope.char(at));
    if has_cased(before) && !has_cased(after) {
        'ς'
    } else {
        'σ'
    }
}

fn has_cased(chars: impl Iterator<Item = char>) -> bool {
    for ch in chars {
        if is_case_ignorable(ch) {
            continue;
        }
        return is_cased(ch);
    }
    false
}

fn is_word_char(ch: char) -> bool {
    char_matches(ch, class_re(r"\A\w\z", 0))
}

fn is_cased(ch: char) -> bool {
    char_matches(ch, class_re(r"\A\p{Cased}\z", 1))
}

fn is_case_ignorable(ch: char) -> bool {
    char_matches(ch, class_re(r"\A\p{Case_Ignorable}\z", 2))
}

fn char_matches(ch: char, re: &regex::Regex) -> bool {
    let mut buf = [0u8; 4];
    re.is_match(ch.encode_utf8(&mut buf))
}

fn class_re(pattern: &'static str, slot: usize) -> &'static regex::Regex {
    static SLOTS: [OnceLock<regex::Regex>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    SLOTS[slot]
        .get_or_init(|| regex::Regex::new(pattern).expect("character class pattern is valid"))
}
