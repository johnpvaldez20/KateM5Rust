// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Escape expansion for patterns and replacement text.

use crate::{Match, SearchError};
use kate_text::CharRange;
use ropey::Rope;

/// Expand a plain-search pattern.
///
/// Valid escapes are `\\`, `\n`, `\t`, `\xNN`, and `\uNNNN`.
/// `at` in [`SearchError::InvalidEscape`] is the byte index of the `\`.
pub(crate) fn expand_pattern(pattern: &str) -> Result<String, SearchError> {
    let mut out = String::new();
    let mut index = 0;
    while index < pattern.len() {
        let ch = char_at(pattern, index);
        if ch != '\\' {
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        index = push_pattern_escape(pattern, index, &mut out)?;
    }
    Ok(out)
}

pub(crate) fn expand(
    replacement: &str,
    hit: &Match,
    rope: &Rope,
    names: Option<&[Option<String>]>,
) -> String {
    let chars: Vec<char> = replacement.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '\\' => index = push_backslash(&chars, index, hit, rope, &mut out),
            '$' => index = push_dollar(&chars, index, hit, rope, names, &mut out),
            ch => {
                out.push(ch);
                index += 1;
            }
        }
    }
    out
}

pub(crate) fn slice_text(rope: &Rope, range: CharRange) -> String {
    let len = rope.len_chars();
    let start = range.start().0.min(len);
    let end = range.end().0.min(len).max(start);
    if start == end {
        return String::new();
    }
    rope.slice(start..end).to_string()
}

fn char_at(text: &str, index: usize) -> char {
    text[index..].chars().next().unwrap_or('\0')
}

fn push_pattern_escape(pattern: &str, at: usize, out: &mut String) -> Result<usize, SearchError> {
    let Some(kind) = pattern[at..].chars().nth(1) else {
        return Err(SearchError::InvalidEscape { at });
    };
    let body = at + 1 + kind.len_utf8();
    match kind {
        '\\' => {
            out.push('\\');
            Ok(body)
        }
        'n' => {
            out.push('\n');
            Ok(body)
        }
        't' => {
            out.push('\t');
            Ok(body)
        }
        'x' => push_hex(pattern, at, body, 2, out),
        'u' => push_hex(pattern, at, body, 4, out),
        _ => Err(SearchError::InvalidEscape { at }),
    }
}

fn push_hex(
    pattern: &str,
    at: usize,
    body: usize,
    digits: usize,
    out: &mut String,
) -> Result<usize, SearchError> {
    let Some(hex) = take_hex(pattern, body, digits) else {
        return Err(SearchError::InvalidEscape { at });
    };
    let Some(ch) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
        return Err(SearchError::InvalidEscape { at });
    };
    out.push(ch);
    Ok(body + hex.len())
}

fn take_hex(pattern: &str, from: usize, digits: usize) -> Option<&str> {
    let rest = pattern.get(from..)?;
    let mut end = 0;
    let mut count = 0;
    for ch in rest.chars() {
        if count == digits {
            break;
        }
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        end += ch.len_utf8();
        count += 1;
    }
    if count == digits {
        Some(&rest[..end])
    } else {
        None
    }
}

fn push_backslash(
    chars: &[char],
    index: usize,
    hit: &Match,
    rope: &Rope,
    out: &mut String,
) -> usize {
    let Some(kind) = chars.get(index + 1).copied() else {
        out.push('\\');
        return index + 1;
    };
    match kind {
        '\\' => out.push('\\'),
        'n' => out.push('\n'),
        't' => out.push('\t'),
        '0'..='9' => {
            let group = digit(kind);
            if !append_group(group, hit, rope, out) {
                out.push(kind);
            }
        }
        other => out.push(other),
    }
    index + 2
}

fn push_dollar(
    chars: &[char],
    index: usize,
    hit: &Match,
    rope: &Rope,
    names: Option<&[Option<String>]>,
    out: &mut String,
) -> usize {
    let Some(kind) = chars.get(index + 1).copied() else {
        out.push('$');
        return index + 1;
    };
    match kind {
        '$' => {
            out.push('$');
            index + 2
        }
        '{' => push_braced(chars, index, hit, rope, names, out),
        ch if ch.is_ascii_digit() => {
            append_group(digit(ch), hit, rope, out);
            index + 2
        }
        _ => {
            out.push('$');
            index + 1
        }
    }
}

fn push_braced(
    chars: &[char],
    index: usize,
    hit: &Match,
    rope: &Rope,
    names: Option<&[Option<String>]>,
    out: &mut String,
) -> usize {
    let body_at = index + 2;
    let Some(rel) = chars[body_at..].iter().position(|ch| *ch == '}') else {
        out.push('$');
        return index + 1;
    };
    let body: String = chars[body_at..body_at + rel].iter().collect();
    if let Some(group) = parse_index(&body) {
        append_group(group, hit, rope, out);
    } else {
        push_name(&body, hit, rope, names, out);
    }
    body_at + rel + 1
}

fn push_name(
    name: &str,
    hit: &Match,
    rope: &Rope,
    names: Option<&[Option<String>]>,
    out: &mut String,
) {
    let Some(names) = names else {
        out.push_str("${");
        out.push_str(name);
        out.push('}');
        return;
    };
    let Some(group) = names.iter().position(|item| item.as_deref() == Some(name)) else {
        return;
    };
    append_group(group, hit, rope, out);
}

fn append_group(index: usize, hit: &Match, rope: &Rope, out: &mut String) -> bool {
    match hit.captures.get(index) {
        Some(Some(range)) => {
            out.push_str(&slice_text(rope, *range));
            true
        }
        Some(None) => true,
        None => false,
    }
}

fn digit(ch: char) -> usize {
    usize::from(u8::try_from(ch).unwrap_or(b'0') - b'0')
}

fn parse_index(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
