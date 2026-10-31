// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! PCRE-flavoured patterns from syntax definitions, compiled once.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use fancy_regex::Regex;
use lru::LruCache;

use crate::definition::{ContextId, ContextSwitch, DefId, RuleKind};
use crate::repository::Repository;

/// PCRE-flavoured patterns from the XML, compiled once. `fancy-regex` handles lookaround and
/// backreferences; patterns it cannot compile are reported at load, not at match time.
pub struct RegexCache {
    by_rule: HashMap<(ContextId, u16), Regex>,
    dynamic: LruCache<String, Regex>,
}

/// Translates QRegularExpression syntax that differs from fancy-regex (POSIX classes, \h, \R, (?i) placement) and
/// expands `%1`..`%9` from captures for dynamic rules.
pub fn translate_pattern(
    pcre: &str,
    insensitive: bool,
    minimal: bool,
    captures: Option<&[String]>,
) -> Result<String, RegexError> {
    let substituted = match captures {
        Some(caps) => replace_captures(pcre, caps, true),
        None => pcre.to_string(),
    };
    let mut translated = translate_pcre(&substituted)?;
    if minimal {
        translated = invert_greed(&translated);
    }
    if insensitive && !has_leading_case_flag(&translated) {
        translated.insert_str(0, "(?i)");
    }
    Ok(translated)
}

#[derive(Debug)]
pub enum RegexError {
    Unsupported { pattern: String, reason: String },
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { pattern, reason } => {
                write!(f, "unsupported pattern {pattern}: {reason}")
            }
        }
    }
}

impl std::error::Error for RegexError {}

impl RegexCache {
    pub(crate) fn compile(repo: &Repository, def: DefId) -> Result<Self, RegexError> {
        let mut by_rule = HashMap::new();
        for context_id in reachable_contexts(repo, def) {
            let context = repo.context(context_id);
            for (index, rule) in context.rules.iter().enumerate() {
                let RuleKind::RegExpr {
                    pattern,
                    insensitive,
                    minimal,
                } = &rule.kind
                else {
                    continue;
                };
                let index = u16::try_from(index).map_err(|_| RegexError::Unsupported {
                    pattern: pattern.clone(),
                    reason: "a context has more than 65535 rules".to_string(),
                })?;
                let translated = translate_pattern(pattern, *insensitive, *minimal, None)?;
                if rule.dynamic {
                    // Dynamic rules are compiled when captures exist. Reject a static verb here.
                    continue;
                }
                let regex = compile_regex(&translated)?;
                by_rule.insert((context_id, index), regex);
            }
        }
        let capacity = NonZeroUsize::new(128).expect("128 is not zero");
        Ok(Self {
            by_rule,
            dynamic: LruCache::new(capacity),
        })
    }

    pub(crate) fn static_regex(&self, context: ContextId, index: u16) -> Option<&Regex> {
        self.by_rule.get(&(context, index))
    }

    pub(crate) fn dynamic_regex(
        &mut self,
        pattern: &str,
        insensitive: bool,
        minimal: bool,
        captures: &[String],
    ) -> Result<Option<&Regex>, RegexError> {
        let translated = translate_pattern(pattern, insensitive, minimal, Some(captures))?;
        if !self.dynamic.contains(&translated) {
            let regex = compile_regex(&translated)?;
            self.dynamic.put(translated.clone(), regex);
        }
        Ok(self.dynamic.get(&translated))
    }
}

fn compile_regex(pattern: &str) -> Result<Regex, RegexError> {
    Regex::new(pattern).map_err(|err| RegexError::Unsupported {
        pattern: pattern.to_string(),
        reason: err.to_string(),
    })
}

fn reachable_contexts(repo: &Repository, def: DefId) -> Vec<ContextId> {
    let range = repo.definition(def).contexts.clone();
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    let mut stack: Vec<ContextId> = (range.start..range.end).map(ContextId).collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        order.push(id);
        let context = repo.context(id);
        for rule in &context.rules {
            push_switch(&mut stack, rule.switch);
        }
        push_switch(&mut stack, context.line_end);
        push_switch(&mut stack, context.line_empty);
        if let Some(switch) = context.fallthrough {
            push_switch(&mut stack, switch);
        }
    }
    order
}

fn push_switch(stack: &mut Vec<ContextId>, switch: ContextSwitch) {
    match switch {
        ContextSwitch::Push(id) | ContextSwitch::PopPush(_, id) => stack.push(id),
        ContextSwitch::Stay | ContextSwitch::Pop(_) => {}
    }
}

fn replace_captures(pattern: &str, captures: &[String], quote: bool) -> String {
    let mut result = pattern.to_string();
    let last = captures.len().min(9);
    for index in (1..=last).rev() {
        let token = format!("%{index}");
        let value = &captures[index - 1];
        let replacement = if quote {
            fancy_regex::escape(value).into_owned()
        } else {
            value.clone()
        };
        result = result.replace(&token, &replacement);
    }
    result
}

fn has_leading_case_flag(pattern: &str) -> bool {
    let rest = pattern.strip_prefix("(?");
    let Some(rest) = rest else {
        return false;
    };
    rest.starts_with('i') || rest.starts_with("-i")
}

const H_SPACE: &str =
    "\\t\\u{0020}\\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{202F}\\u{205F}\\u{3000}";
const R_BREAK: &str = "(?:\\r\\n|[\\n\\x0B\\x0C\\r\\u{0085}\\u{2028}\\u{2029}])";

fn translate_pcre(pattern: &str) -> Result<String, RegexError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    let mut in_class = false;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            index = push_escape(&chars, index, in_class, pattern, &mut out)?;
            continue;
        }
        if !in_class && ch == '(' && chars.get(index + 1) == Some(&'*') {
            return Err(RegexError::Unsupported {
                pattern: pattern.to_string(),
                reason: "PCRE verb".to_string(),
            });
        }
        if !in_class && ch == '[' {
            in_class = true;
            out.push(ch);
            index += 1;
            index = skip_class_opener(&chars, index, &mut out);
            continue;
        }
        if in_class && ch == '[' && chars.get(index + 1) == Some(&':') {
            index = copy_posix(&chars, index, &mut out);
            continue;
        }
        if in_class && ch == ']' {
            in_class = false;
        }
        out.push(ch);
        index += 1;
    }
    Ok(out)
}

fn push_escape(
    chars: &[char],
    index: usize,
    in_class: bool,
    pattern: &str,
    out: &mut String,
) -> Result<usize, RegexError> {
    let Some(next) = chars.get(index + 1).copied() else {
        out.push('\\');
        return Ok(index + 1);
    };
    match next {
        'h' => {
            if in_class {
                out.push_str(H_SPACE);
            } else {
                out.push('[');
                out.push_str(H_SPACE);
                out.push(']');
            }
        }
        'H' => {
            if in_class {
                return Err(RegexError::Unsupported {
                    pattern: pattern.to_string(),
                    reason: "\\H inside a character class".to_string(),
                });
            }
            out.push('[');
            out.push('^');
            out.push_str(H_SPACE);
            out.push(']');
        }
        'R' => {
            if in_class {
                return Err(RegexError::Unsupported {
                    pattern: pattern.to_string(),
                    reason: "\\R inside a character class".to_string(),
                });
            }
            out.push_str(R_BREAK);
        }
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    Ok(index + 2)
}

fn skip_class_opener(chars: &[char], mut index: usize, out: &mut String) -> usize {
    if chars.get(index) == Some(&'^') {
        out.push('^');
        index += 1;
    }
    if chars.get(index) == Some(&']') {
        out.push(']');
        index += 1;
    }
    index
}

fn copy_posix(chars: &[char], index: usize, out: &mut String) -> usize {
    let mut cursor = index;
    while cursor < chars.len() {
        out.push(chars[cursor]);
        if chars[cursor] == ']' && cursor > index && chars[cursor - 1] == ':' {
            return cursor + 1;
        }
        cursor += 1;
    }
    cursor
}

fn invert_greed(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    let mut in_class = false;
    let mut after_atom = false;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            out.push('\\');
            index += 1;
            if index < chars.len() {
                out.push(chars[index]);
                index += 1;
            }
            if !in_class {
                after_atom = true;
            }
            continue;
        }
        if !in_class && ch == '[' {
            in_class = true;
            after_atom = false;
            out.push(ch);
            index += 1;
            index = skip_class_opener(&chars, index, &mut out);
            continue;
        }
        if in_class && ch == '[' && chars.get(index + 1) == Some(&':') {
            index = copy_posix(&chars, index, &mut out);
            continue;
        }
        if in_class && ch == ']' {
            in_class = false;
            after_atom = true;
            out.push(ch);
            index += 1;
            continue;
        }
        if in_class {
            out.push(ch);
            index += 1;
            continue;
        }
        if after_atom && is_quantifier(ch, &chars[index..]) {
            index = rewrite_quantifier(&chars, index, &mut out);
            after_atom = false;
            continue;
        }
        out.push(ch);
        after_atom = atom_follows(ch);
        index += 1;
    }
    out
}

fn atom_follows(ch: char) -> bool {
    !matches!(ch, '|' | '(')
}

fn is_quantifier(ch: char, rest: &[char]) -> bool {
    match ch {
        '*' | '+' | '?' => true,
        '{' => rest.get(1).is_some_and(|next| next.is_ascii_digit()),
        _ => false,
    }
}

fn rewrite_quantifier(chars: &[char], index: usize, out: &mut String) -> usize {
    let mut cursor = index;
    out.push(chars[cursor]);
    if chars[cursor] == '{' {
        cursor += 1;
        while cursor < chars.len() {
            out.push(chars[cursor]);
            if chars[cursor] == '}' {
                cursor += 1;
                break;
            }
            cursor += 1;
        }
    } else {
        cursor += 1;
    }
    match chars.get(cursor).copied() {
        Some('+') => {
            out.push('+');
            cursor + 1
        }
        Some('?') => cursor + 1,
        _ => {
            out.push('?');
            cursor
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RegexError, translate_pattern};
    use crate::Highlighter;
    use crate::repository::Repository;

    const SKIP_XML: &str = r##"
<language name="Skip" section="Test" version="1" kateversion="5.0" extensions="*.skip" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay">
        <RegExpr String="(*SKIP)a" context="#stay" attribute="Normal"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    #[test]
    fn unsupported_pattern_is_reported_at_load() {
        let err = translate_pattern("(*SKIP)a", false, false, None).expect_err("verb");
        assert!(matches!(err, RegexError::Unsupported { .. }));

        let mut repo = Repository::empty();
        repo.add_xml(SKIP_XML).expect("parse");
        repo.link().expect("link");
        let id = repo.by_name("Skip").expect("name");
        let loaded = Highlighter::new(&repo, id);
        assert!(matches!(loaded, Err(RegexError::Unsupported { .. })));
    }

    #[test]
    fn minimal_swaps_greedy_quantifiers() {
        let swapped = translate_pattern("a+b*?", false, true, None).expect("pattern");
        assert_eq!(swapped, "a+?b*");
        let flagged = translate_pattern("a+", true, false, None).expect("pattern");
        assert_eq!(flagged, "(?i)a+");
    }

    #[test]
    fn horizontal_space_is_not_hex() {
        let pattern = translate_pattern(r"\h+", false, false, None).expect("pattern");
        assert!(!pattern.contains(r"\h"), "{pattern}");
        assert!(pattern.contains(r"\t"), "{pattern}");
    }
}
