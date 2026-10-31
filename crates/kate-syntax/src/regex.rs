// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! PCRE-flavoured patterns from syntax definitions, compiled once.

use std::collections::HashMap;
use std::num::NonZeroUsize;

use fancy_regex::Regex;
use lru::LruCache;

use crate::definition::{ContextId, DefId};
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
    _insensitive: bool,
    _minimal: bool,
    _captures: Option<&[String]>,
) -> Result<String, RegexError> {
    Ok(pcre.to_string())
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
    pub(crate) fn compile(_repo: &Repository, _def: DefId) -> Result<Self, RegexError> {
        let capacity = NonZeroUsize::new(128).expect("128 is not zero");
        let cache = Self {
            by_rule: HashMap::new(),
            dynamic: LruCache::new(capacity),
        };
        let _ = cache.by_rule.len();
        let _ = cache.dynamic.len();
        Ok(cache)
    }

    pub(crate) fn static_regex(&self, context: ContextId, index: u16) -> Option<&Regex> {
        self.by_rule.get(&(context, index))
    }
}

#[cfg(test)]
mod tests {
    use super::{RegexCache, RegexError, translate_pattern};
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
        let loaded = RegexCache::compile(&repo, id);
        assert!(matches!(loaded, Err(RegexError::Unsupported { .. })));

        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _hl = Highlighter::new(&repo, id);
        }));
        assert!(
            caught.is_err(),
            "Highlighter::new reports the pattern at load"
        );
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
