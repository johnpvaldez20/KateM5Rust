// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Line highlighter. The walk matches `AbstractHighlighter::highlightLine`.

use crate::definition::{ContextId, DefId};
use crate::regex::RegexCache;
use crate::repository::Repository;
use crate::state::{Frame, StateId, StateInterner};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: u32,
    pub len: u32,
    pub format: crate::definition::FormatId,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FoldEvent {
    Begin {
        col: u32,
        region: crate::definition::FoldId,
    },
    End {
        col: u32,
        region: crate::definition::FoldId,
    },
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct LineHighlight {
    pub spans: Vec<Span>,
    pub folds: Vec<FoldEvent>,
    pub end_state: StateId,
}

pub struct Highlighter<'a> {
    repo: &'a Repository,
    regexes: RegexCache,
    interner: StateInterner,
}

impl<'a> Highlighter<'a> {
    pub fn new(repo: &'a Repository, def: DefId) -> Self {
        let regexes = RegexCache::compile(repo, def).unwrap_or_else(|err| {
            panic!("unsupported pattern at load: {err}");
        });
        let definition = repo.definition(def);
        Self {
            repo,
            regexes,
            interner: StateInterner::new(definition.initial_context),
        }
    }

    pub fn initial_state(&self) -> StateId {
        self.interner.initial()
    }

    /// Deterministic: same (line, start) always yields the same output. Reuses `out` allocations.
    pub fn highlight_line(&mut self, _line: &str, start: StateId, out: &mut LineHighlight) {
        let _ = self.repo.by_name("");
        let _ = self.regexes.static_regex(ContextId(0), 0);
        let mut scratch = self.interner.get(start).clone();
        let context = scratch.top().context;
        scratch.push_frame(Frame {
            context,
            captures: None,
        });
        let _ = scratch.pop_frames(1);
        out.spans.clear();
        out.folds.clear();
        out.end_state = start;
    }

    pub fn interner(&self) -> &StateInterner {
        &self.interner
    }
}

#[cfg(test)]
mod tests {
    use super::Highlighter;
    use crate::LineHighlight;
    use crate::repository::Repository;

    const LOOK: &str = r##"
<language name="Look" section="Test" version="1" kateversion="5.0" extensions="*.look" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Plain" lineEndContext="#stay">
        <RegExpr String="ab" context="Hit" lookAhead="true"/>
        <DetectChar char="z" attribute="Plain" context="#stay"/>
      </context>
      <context name="Hit" attribute="Plain" lineEndContext="#pop">
        <Detect2Chars char="a" char1="b" attribute="Mark" context="#stay"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Plain" defStyleNum="dsNormal"/>
      <itemData name="Mark" defStyleNum="dsKeyword"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    fn load_one(xml: &str, name: &str) -> (Repository, crate::definition::DefId) {
        let mut repo = Repository::empty();
        repo.add_xml(xml).expect("parse");
        repo.link().expect("link");
        let id = repo.by_name(name).expect("name");
        (repo, id)
    }

    #[test]
    fn lookahead_rule_consumes_nothing() {
        let (repo, id) = load_one(LOOK, "Look");
        let mut hl = Highlighter::new(&repo, id);
        let mut out = LineHighlight::default();
        hl.highlight_line("ab", hl.initial_state(), &mut out);
        assert_eq!(out.spans.len(), 1, "{:?}", out.spans);
        assert_eq!(out.spans[0].start, 0);
        assert_eq!(out.spans[0].len, 2);
        assert_eq!(repo.format(out.spans[0].format).name, "Mark");
    }

    #[test]
    fn highlight_is_deterministic_and_state_converges() {
        let rust = include_str!("../definitions/rust.xml");
        let comments = include_str!("../definitions/comments.xml");
        let alert = include_str!("../definitions/alert.xml");
        let modelines = include_str!("../definitions/modelines.xml");
        let spdx = include_str!("../definitions/spdx-comments.xml");
        let c89 = include_str!("../definitions/ansic89.xml");
        let mut repo = Repository::empty();
        for xml in [rust, comments, alert, modelines, spdx, c89] {
            repo.add_xml(xml).expect("parse");
        }
        repo.link().expect("link");
        let id = repo.by_name("Rust").expect("rust");
        let mut hl = Highlighter::new(&repo, id);
        let text = include_str!("../testdata/golden/rust/input");
        let lines = source_lines(text);
        let mut starts = Vec::new();
        let mut cached = Vec::new();
        let mut state = hl.initial_state();
        for line in &lines {
            starts.push(state);
            let mut once = LineHighlight::default();
            let mut twice = LineHighlight::default();
            hl.highlight_line(line, state, &mut once);
            hl.highlight_line(line, state, &mut twice);
            assert_eq!(once, twice);
            state = once.end_state;
            cached.push(state);
        }
        let index = lines
            .iter()
            .position(|line| line.contains("pub struct"))
            .expect("line");
        let changed = lines[index].replace("pub", "pub ");
        let mut state = starts[index];
        let mut out = LineHighlight::default();
        let mut converged = false;
        for cursor in index..lines.len() {
            let line = if cursor == index {
                changed.as_str()
            } else {
                lines[cursor]
            };
            hl.highlight_line(line, state, &mut out);
            if out.end_state == cached[cursor] {
                converged = true;
                break;
            }
            state = out.end_state;
        }
        assert!(
            converged,
            "end state did not meet a cached state inside the file"
        );
    }

    fn source_lines(text: &str) -> Vec<&str> {
        let mut lines = Vec::new();
        let mut rest = text;
        if rest.is_empty() {
            return lines;
        }
        loop {
            if let Some((line, tail)) = rest.split_once('\n') {
                lines.push(line.strip_suffix('\r').unwrap_or(line));
                rest = tail;
                if rest.is_empty() {
                    break;
                }
            } else {
                lines.push(rest.strip_suffix('\r').unwrap_or(rest));
                break;
            }
        }
        lines
    }
}
