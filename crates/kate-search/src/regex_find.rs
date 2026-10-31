// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Regex search over a rope. The engine reads chunks. It does not copy the buffer.

use crate::{Match, SearchError};
use kate_text::{CharRange, Offset};
use regex_cursor::Input;
use regex_cursor::engines::meta::Regex as CursorRegex;
use regex_cursor::regex_automata::util::captures::Captures;
use regex_cursor::regex_automata::util::syntax;
use ropey::Rope;

pub(crate) fn find(
    rope: &Rope,
    pattern: &str,
    case_sensitive: bool,
    start: usize,
    end: usize,
) -> Result<Vec<Match>, SearchError> {
    compile_std(pattern, case_sensitive)?;
    let compiled = compile_cursor(pattern, case_sensitive)?;
    let Ok(start_b) = rope.try_char_to_byte(start) else {
        return Ok(Vec::new());
    };
    let Ok(end_b) = rope.try_char_to_byte(end) else {
        return Ok(Vec::new());
    };
    if start_b > end_b {
        return Ok(Vec::new());
    }
    let mut input = Input::new(rope);
    input.set_span(start_b..end_b);
    let mut found = Vec::new();
    for caps in compiled.captures_iter(input) {
        push_hit(&mut found, rope, &caps, start, end);
    }
    Ok(found)
}

pub(crate) fn capture_name_list(
    pattern: &str,
    case_sensitive: bool,
) -> Result<Vec<Option<String>>, SearchError> {
    let compiled = compile_std(pattern, case_sensitive)?;
    Ok(compiled
        .capture_names()
        .map(|name| name.map(str::to_string))
        .collect())
}

fn compile_std(pattern: &str, case_sensitive: bool) -> Result<regex::Regex, SearchError> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .dot_matches_new_line(false)
        .build()
        .map_err(|err| SearchError::InvalidRegex(err.to_string()))
}

fn compile_cursor(pattern: &str, case_sensitive: bool) -> Result<CursorRegex, SearchError> {
    let config = syntax::Config::new()
        .case_insensitive(!case_sensitive)
        .multi_line(true)
        .dot_matches_new_line(false);
    CursorRegex::builder()
        .syntax(config)
        .build(pattern)
        .map_err(|err| SearchError::InvalidRegex(err.to_string()))
}

fn push_hit(found: &mut Vec<Match>, rope: &Rope, caps: &Captures, start: usize, end: usize) {
    let Some(whole) = group_range(rope, caps, 0) else {
        return;
    };
    if whole.start().0 < start || whole.end().0 > end {
        return;
    }
    let mut captures = Vec::new();
    for index in 0..caps.group_len() {
        captures.push(group_range(rope, caps, index));
    }
    if captures.is_empty() {
        captures.push(Some(whole));
    }
    found.push(Match {
        range: whole,
        captures,
    });
}

fn group_range(rope: &Rope, caps: &Captures, index: usize) -> Option<CharRange> {
    let span = caps.get_group(index)?;
    let start = rope.try_byte_to_char(span.start).ok()?;
    let end = rope.try_byte_to_char(span.end).ok()?;
    if start > end {
        return None;
    }
    Some(CharRange::new(Offset(start), Offset(end)))
}
