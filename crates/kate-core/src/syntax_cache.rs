// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Per-line highlight state for one document.
//!
//! `on_transaction` updates the line vector and marks the edited lines dirty.
//! `ensure` highlights from the first dirty line.
//! It stops when a line end state matches the cache and every forced line is done.
//! It does not highlight the rest of the document.

use std::ops::Range;

use kate_syntax::{DefId, Highlighter, LineHighlight, StateId};
use kate_text::{Edit, LineCol, TextBuffer, Transaction};

/// A state that has not been highlighted.
const MISSING: StateId = StateId(u32::MAX);

/// Per-line end states. `len()` matches the buffer after `fit` or `ensure`.
#[derive(Clone, Debug)]
pub struct SyntaxCache {
    /// Syntax definition. `None` means plain text.
    pub definition: Option<DefId>,
    line_end_states: Vec<StateId>,
    /// First line whose cached state may be wrong.
    dirty_from: Option<u32>,
    /// Last line that this edit must visit before convergence can stop.
    dirty_to: Option<u32>,
}

/// One line-span replacement recorded for undo.
#[derive(Clone, Debug)]
pub(crate) struct LineSplice {
    pub pre_at: u32,
    pub post_at: u32,
    pub old_states: Vec<StateId>,
    pub new_len: u32,
}

/// Dirty flags saved around a failed apply.
#[derive(Copy, Clone, Debug)]
pub(crate) struct DirtySnap {
    from: Option<u32>,
    to: Option<u32>,
}

impl SyntaxCache {
    /// Empty cache for `definition`. The first `ensure` highlights from line 0.
    pub fn new(definition: Option<DefId>) -> Self {
        Self {
            definition,
            line_end_states: Vec::new(),
            dirty_from: Some(0),
            dirty_to: None,
        }
    }

    /// Grow or shrink the state vector to `lines`.
    pub(crate) fn fit(&mut self, lines: u32) {
        let target = lines.max(1) as usize;
        let current = self.line_end_states.len();
        if current == target {
            return;
        }
        if current < target {
            let start = current as u32;
            self.line_end_states.resize(target, MISSING);
            let end = (target - 1) as u32;
            self.mark_dirty(start, end);
            return;
        }
        self.line_end_states.truncate(target);
        if self.dirty_from.is_some_and(|line| line as usize >= target) {
            self.dirty_from = None;
            self.dirty_to = None;
        } else if self.dirty_to.is_some_and(|line| line as usize >= target) {
            self.dirty_to = Some((target - 1) as u32);
        }
    }

    /// Replace the definition and drop cached states.
    pub(crate) fn set_definition(&mut self, definition: Option<DefId>) {
        if self.definition == definition {
            return;
        }
        self.definition = definition;
        self.invalidate_all();
    }

    /// Mark every cached line dirty.
    pub(crate) fn invalidate_all(&mut self) {
        self.line_end_states.fill(MISSING);
        if self.line_end_states.is_empty() {
            self.dirty_from = Some(0);
            self.dirty_to = None;
            return;
        }
        let last = (self.line_end_states.len() - 1) as u32;
        self.dirty_from = Some(0);
        self.dirty_to = Some(last);
    }

    /// Splice the state vector to the post-transaction line count.
    pub fn on_transaction(&mut self, before: &TextBuffer, tx: &Transaction) {
        let _ = self.plan_and_commit(before, tx);
    }

    /// Plan the splice, apply it, and return the plan plus the previous dirty flags.
    pub(crate) fn plan_and_commit(
        &mut self,
        before: &TextBuffer,
        tx: &Transaction,
    ) -> (Vec<LineSplice>, DirtySnap) {
        let snap = self.snapshot_dirty();
        self.fit(before.line_count());
        let splices = self.plan(before, tx);
        self.commit_forward(&splices);
        (splices, snap)
    }

    pub(crate) fn snapshot_dirty(&self) -> DirtySnap {
        DirtySnap {
            from: self.dirty_from,
            to: self.dirty_to,
        }
    }

    pub(crate) fn restore_dirty(&mut self, snap: DirtySnap) {
        self.dirty_from = snap.from;
        self.dirty_to = snap.to;
    }

    /// Undo one committed plan.
    pub(crate) fn commit_reverse(&mut self, splices: &[LineSplice]) {
        if splices.is_empty() {
            return;
        }
        if splices.iter().all(LineSplice::trivial) {
            self.restore_trivial(splices);
        } else {
            self.rebuild_reverse(splices);
        }
        self.mark_span(splices, true);
    }

    /// Highlight from the dirty line through `through_line`.
    ///
    /// The result is the lines whose cached state was rewritten.
    /// The cache stores end states, not spans.
    /// A line inside the result may have the same spans as before.
    /// Lines after convergence are not part of the result.
    pub fn ensure(
        &mut self,
        buf: &TextBuffer,
        hl: &mut Highlighter<'_>,
        through_line: u32,
    ) -> Range<u32> {
        if self.definition.is_none() {
            return 0..0;
        }
        self.fit(buf.line_count());
        let Some(start) = self.dirty_from else {
            return 0..0;
        };
        let last = buf.line_count().saturating_sub(1);
        let limit = through_line.min(last);
        if start > limit {
            return 0..0;
        }
        self.paint(buf, hl, start, limit, last)
    }

    /// State at the start of `line`.
    ///
    /// Line 0 starts at [`StateId`] `0`.
    /// A line that has not been highlighted also returns `0`.
    pub fn start_state(&self, line: u32) -> StateId {
        if line == 0 {
            return StateId(0);
        }
        match self.line_end_states.get((line as usize) - 1).copied() {
            Some(state) if state != MISSING => state,
            _ => StateId(0),
        }
    }

    fn plan(&self, before: &TextBuffer, tx: &Transaction) -> Vec<LineSplice> {
        let regions = regions_of(before, tx);
        let mut shift = 0i32;
        let mut splices = Vec::with_capacity(regions.len());
        for region in regions {
            let pre = region.start;
            let old_len = region.old_count as usize;
            let from = pre as usize;
            let mut old_states = Vec::with_capacity(old_len);
            for index in 0..old_len {
                let state = self
                    .line_end_states
                    .get(from + index)
                    .copied()
                    .unwrap_or(MISSING);
                old_states.push(state);
            }
            let post = add_delta(pre, shift);
            shift += region.new_count as i32 - region.old_count as i32;
            splices.push(LineSplice {
                pre_at: pre,
                post_at: post,
                old_states,
                new_len: region.new_count,
            });
        }
        splices
    }

    pub(crate) fn commit_forward(&mut self, splices: &[LineSplice]) {
        if splices.is_empty() {
            return;
        }
        if !splices.iter().all(LineSplice::trivial) {
            self.rebuild_forward(splices);
        }
        self.mark_span(splices, false);
    }

    fn rebuild_forward(&mut self, splices: &[LineSplice]) {
        let mut out = Vec::with_capacity(self.line_end_states.len() + splices.len());
        let mut cursor = 0usize;
        for splice in splices {
            let pre = splice.pre_at as usize;
            self.copy_until(&mut out, &mut cursor, pre);
            out.extend(fill_forward(&splice.old_states, splice.new_len));
            cursor = pre.saturating_add(splice.old_states.len());
        }
        self.copy_rest(&mut out, cursor);
        self.line_end_states = out;
    }

    fn rebuild_reverse(&mut self, splices: &[LineSplice]) {
        let mut out = Vec::with_capacity(self.line_end_states.len());
        let mut cursor = 0usize;
        for splice in splices {
            let post = splice.post_at as usize;
            self.copy_until(&mut out, &mut cursor, post);
            out.extend_from_slice(&splice.old_states);
            cursor = post.saturating_add(splice.new_len as usize);
        }
        self.copy_rest(&mut out, cursor);
        self.line_end_states = out;
    }

    fn restore_trivial(&mut self, splices: &[LineSplice]) {
        for splice in splices {
            let Some(state) = splice.old_states.first().copied() else {
                continue;
            };
            let index = splice.post_at as usize;
            if let Some(slot) = self.line_end_states.get_mut(index) {
                *slot = state;
            }
        }
    }

    fn copy_until(&self, out: &mut Vec<StateId>, cursor: &mut usize, stop: usize) {
        if *cursor >= stop {
            return;
        }
        let end = stop.min(self.line_end_states.len());
        if *cursor < end {
            out.extend_from_slice(&self.line_end_states[*cursor..end]);
        }
        *cursor = stop;
    }

    fn copy_rest(&self, out: &mut Vec<StateId>, cursor: usize) {
        if cursor < self.line_end_states.len() {
            out.extend_from_slice(&self.line_end_states[cursor..]);
        }
    }

    fn mark_span(&mut self, splices: &[LineSplice], restored: bool) {
        let Some(first) = splices.first() else {
            return;
        };
        let start = if restored {
            splices
                .iter()
                .map(|splice| splice.pre_at)
                .min()
                .unwrap_or(first.pre_at)
        } else {
            first.pre_at
        };
        let end = splices
            .iter()
            .map(|splice| {
                let at = if restored {
                    splice.pre_at
                } else {
                    splice.post_at
                };
                let len = if restored {
                    splice.old_states.len() as u32
                } else {
                    splice.new_len
                };
                at.saturating_add(len.saturating_sub(1))
            })
            .max()
            .unwrap_or(start);
        self.mark_dirty(start, end);
    }

    fn mark_dirty(&mut self, start: u32, end_inclusive: u32) {
        let end = end_inclusive.max(start);
        self.dirty_from = Some(match self.dirty_from {
            Some(line) => line.min(start),
            None => start,
        });
        self.dirty_to = Some(match self.dirty_to {
            Some(line) => line.max(end),
            None => end,
        });
    }

    fn paint(
        &mut self,
        buf: &TextBuffer,
        hl: &mut Highlighter<'_>,
        start: u32,
        limit: u32,
        last: u32,
    ) -> Range<u32> {
        let must = self.dirty_to.unwrap_or(start).max(start);
        let mut line = start;
        let mut end = start;
        let mut out = LineHighlight::default();
        while line <= limit {
            let cached = self.state_at(line);
            let text = line_text(buf, line);
            hl.highlight_line(&text, self.input_state(line), &mut out);
            self.store(line, out.end_state);
            end = line + 1;
            if cached != MISSING && out.end_state == cached && line >= must {
                self.clear_dirty();
                return start..end;
            }
            if line == limit {
                break;
            }
            line += 1;
        }
        self.finish_partial(end, last);
        start..end
    }

    fn finish_partial(&mut self, next: u32, last: u32) {
        if next > last {
            self.clear_dirty();
            return;
        }
        self.dirty_from = Some(next);
        if self.dirty_to.is_some_and(|end| end < next) {
            self.dirty_to = None;
        }
    }

    fn clear_dirty(&mut self) {
        self.dirty_from = None;
        self.dirty_to = None;
    }

    fn state_at(&self, line: u32) -> StateId {
        self.line_end_states
            .get(line as usize)
            .copied()
            .unwrap_or(MISSING)
    }

    fn input_state(&self, line: u32) -> StateId {
        if line == 0 {
            return StateId(0);
        }
        match self.state_at(line - 1) {
            MISSING => StateId(0),
            state => state,
        }
    }

    fn store(&mut self, line: u32, state: StateId) {
        let index = line as usize;
        if index < self.line_end_states.len() {
            self.line_end_states[index] = state;
        }
    }
}

impl LineSplice {
    fn trivial(&self) -> bool {
        self.old_states.len() == 1 && self.new_len == 1
    }
}

struct Region {
    start: u32,
    old_count: u32,
    new_count: u32,
}

fn regions_of(buf: &TextBuffer, tx: &Transaction) -> Vec<Region> {
    let mut raw = Vec::new();
    for edit in tx.edits() {
        raw.push(region_of(buf, edit));
    }
    merge_regions(raw)
}

fn region_of(buf: &TextBuffer, edit: &Edit) -> Region {
    let start = buf.line_col(edit.start).line;
    let old_nl = count_newlines(&edit.old);
    let new_nl = count_newlines(&edit.new);
    Region {
        start,
        old_count: old_nl.saturating_add(1),
        new_count: new_nl.saturating_add(1),
    }
}

fn merge_regions(raw: Vec<Region>) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    for region in raw {
        if let Some(prev) = out.last_mut()
            && region.start < prev.start.saturating_add(prev.old_count)
        {
            absorb(prev, &region);
            continue;
        }
        out.push(region);
    }
    out
}

fn absorb(prev: &mut Region, region: &Region) {
    let prev_end = prev.start.saturating_add(prev.old_count);
    let next_end = region.start.saturating_add(region.old_count);
    let union_end = prev_end.max(next_end);
    let old_count = union_end.saturating_sub(prev.start).max(1);
    let prev_delta = prev.new_count as i32 - prev.old_count as i32;
    let next_delta = region.new_count as i32 - region.old_count as i32;
    let new_count = add_delta(old_count, prev_delta.saturating_add(next_delta)).max(1);
    prev.old_count = old_count;
    prev.new_count = new_count;
}

fn fill_forward(old: &[StateId], new_len: u32) -> Vec<StateId> {
    let new_len = new_len as usize;
    if new_len == 0 {
        return Vec::new();
    }
    if old.len() == 1 && new_len == 1 {
        return vec![old[0]];
    }
    let follower = old.last().copied().unwrap_or(MISSING);
    let mut out = vec![MISSING; new_len.saturating_sub(1)];
    out.push(follower);
    out
}

fn count_newlines(text: &str) -> u32 {
    let count = text.chars().filter(|ch| *ch == '\n').count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

fn add_delta(line: u32, delta: i32) -> u32 {
    let next = i64::from(line) + i64::from(delta);
    u32::try_from(next.max(0)).unwrap_or(u32::MAX)
}

pub(crate) fn line_text(buf: &TextBuffer, line: u32) -> String {
    let (start, end) = line_bounds(buf, line);
    if start >= end {
        return String::new();
    }
    buf.text().slice(start..end).to_string()
}

pub(crate) fn line_bounds(buf: &TextBuffer, line: u32) -> (usize, usize) {
    let start = buf.offset(LineCol { line, col: 0 }).0;
    let mut end = buf
        .offset(LineCol {
            line,
            col: u32::MAX,
        })
        .0;
    let rope = buf.text();
    if end > start && rope.char(end - 1) == '\r' {
        let at_break = end == rope.len_chars() || rope.char(end) == '\n';
        if at_break {
            end -= 1;
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::{SyntaxCache, line_text};
    use kate_syntax::{Highlighter, Repository};
    use kate_text::{Edit, LineCol, Offset, TextBuffer, Transaction};
    use std::time::{Duration, Instant};

    const RUST_XML: &str = include_str!("../../kate-syntax/definitions/rust.xml");
    const COMMENTS_XML: &str = include_str!("../../kate-syntax/definitions/comments.xml");
    const ALERT_XML: &str = include_str!("../../kate-syntax/definitions/alert.xml");
    const MODELINES_XML: &str = include_str!("../../kate-syntax/definitions/modelines.xml");
    const SPDX_XML: &str = include_str!("../../kate-syntax/definitions/spdx-comments.xml");

    fn rust_repo() -> (Repository, kate_syntax::DefId) {
        let mut repo = Repository::empty();
        for xml in [RUST_XML, COMMENTS_XML, ALERT_XML, MODELINES_XML, SPDX_XML] {
            repo.add_xml(xml).expect("syntax xml parses");
        }
        repo.link().expect("syntax xml links");
        let id = repo.by_name("Rust").expect("Rust definition");
        (repo, id)
    }

    fn tiny_repo() -> (Repository, kate_syntax::DefId) {
        let xml = r##"
<language name="Tiny" section="Test" version="1" kateversion="5.0" extensions="*.tiny" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay">
        <Detect2Chars char="/" char1="*" attribute="Comment" context="Comment"/>
      </context>
      <context name="Comment" attribute="Comment" lineEndContext="#stay">
        <Detect2Chars char="*" char1="/" attribute="Comment" context="#pop"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
      <itemData name="Comment" defStyleNum="dsComment"/>
    </itemDatas>
  </highlighting>
</language>
"##;
        let mut repo = Repository::empty();
        repo.add_xml(xml).expect("tiny xml parses");
        repo.link().expect("tiny xml links");
        let id = repo.by_name("Tiny").expect("Tiny definition");
        (repo, id)
    }

    fn lines_of(count: usize, body: &str) -> String {
        let mut text = String::with_capacity(count * (body.len() + 1));
        for index in 0..count {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(body);
        }
        text
    }

    fn insert_at(buf: &mut TextBuffer, cache: &mut SyntaxCache, line: u32, text: &str) {
        let at = buf.offset(LineCol { line, col: 0 });
        let tx = Transaction::single(Edit::insert(at, text));
        cache.on_transaction(buf, &tx);
        buf.apply(&tx).expect("the edit applies");
    }

    #[test]
    fn rehighlight_stops_when_state_converges() {
        let (repo, id) = rust_repo();
        let mut hl = Highlighter::new(&repo, id).expect("Rust patterns compile");
        let mut buf = TextBuffer::from_str(&lines_of(10_000, "let v = 1;"));
        let mut cache = SyntaxCache::new(Some(id));
        let last = buf.line_count() - 1;
        let first = cache.ensure(&buf, &mut hl, last);
        assert_eq!(first.end, buf.line_count());
        assert!(cache.dirty_from.is_none());
        let saved_tail = cache.line_end_states[last as usize];
        let saved_before = cache.line_end_states[499];

        insert_at(&mut buf, &mut cache, 500, "x");
        let small = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        assert_eq!(small.start, 500);
        assert!(
            small.end - small.start < 20,
            "a plain edit re-highlighted {} lines",
            small.end - small.start
        );
        assert_eq!(cache.line_end_states[last as usize], saved_tail);
        assert_eq!(line_text(&buf, 500), "xlet v = 1;");

        let before_comment = cache.line_end_states[499];
        assert_eq!(before_comment, saved_before);
        insert_at(&mut buf, &mut cache, 500, "/*");
        let comment = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        assert_eq!(comment.start, 500);
        assert_eq!(
            comment.end,
            buf.line_count(),
            "an open block comment must re-highlight every following line"
        );
        assert_ne!(cache.line_end_states[last as usize], saved_tail);
        let oracle = oracle_end(&mut hl, &buf, 500, before_comment, last);
        assert_eq!(cache.line_end_states[last as usize], oracle);
        let mid = oracle_end(&mut hl, &buf, 500, before_comment, 2_500);
        assert_eq!(cache.line_end_states[2_500], mid);
        assert_ne!(mid, saved_tail);
    }

    fn oracle_end(
        hl: &mut Highlighter<'_>,
        buf: &TextBuffer,
        start_line: u32,
        start_state: kate_syntax::StateId,
        target: u32,
    ) -> kate_syntax::StateId {
        let mut state = start_state;
        let mut out = kate_syntax::LineHighlight::default();
        for line in start_line..=target {
            hl.highlight_line(&line_text(buf, line), state, &mut out);
            state = out.end_state;
        }
        state
    }

    #[test]
    fn two_distant_edits_keep_the_gap_in_the_dirty_span() {
        let (repo, id) = tiny_repo();
        let mut hl = Highlighter::new(&repo, id).expect("tiny patterns compile");
        let mut buf = TextBuffer::from_str(&lines_of(80, "code"));
        let mut cache = SyntaxCache::new(Some(id));
        let _ = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        let tx = Transaction::new(vec![
            Edit::insert(buf.offset(LineCol { line: 2, col: 0 }), "x"),
            Edit::insert(buf.offset(LineCol { line: 40, col: 0 }), "y"),
        ])
        .expect("the edits do not overlap");
        cache.on_transaction(&buf, &tx);
        buf.apply(&tx).expect("the edits apply");
        let range = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        assert_eq!(range.start, 2);
        assert!(
            range.end > 40,
            "the second edit must be inside the highlighted span"
        );
        assert!(
            range.end < buf.line_count(),
            "lines after the last edit must stay cached"
        );
    }

    #[test]
    fn incremental_edit_on_a_large_file_stays_local() {
        const LINES: usize = 100_000;
        let (repo, id) = tiny_repo();
        let mut hl = Highlighter::new(&repo, id).expect("tiny patterns compile");
        let mut buf = TextBuffer::from_str(&lines_of(LINES, "code"));
        let mut cache = SyntaxCache::new(Some(id));
        let _ = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        let started = Instant::now();
        insert_at(&mut buf, &mut cache, 50_000, "x");
        let range = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        let elapsed = started.elapsed();
        assert_eq!(range.start, 50_000);
        assert!(range.end - range.start < 5);
        assert!(
            elapsed < Duration::from_secs(10),
            "incremental edit on {LINES} lines took {elapsed:?}"
        );
    }

    #[test]
    fn open_comment_on_a_large_file_highlights_only_the_tail() {
        const LINES: usize = 20_000;
        let (repo, id) = tiny_repo();
        let mut hl = Highlighter::new(&repo, id).expect("tiny patterns compile");
        let mut buf = TextBuffer::from_str(&lines_of(LINES, "code"));
        let mut cache = SyntaxCache::new(Some(id));
        let _ = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        let started = Instant::now();
        insert_at(&mut buf, &mut cache, 19_000, "/*");
        let range = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        let elapsed = started.elapsed();
        assert_eq!(range.start, 19_000);
        assert_eq!(range.end, buf.line_count());
        assert!(range.end - range.start < 2_000);
        assert!(
            elapsed < Duration::from_secs(10),
            "tail re-highlight took {elapsed:?}"
        );
    }

    #[test]
    fn newline_splice_keeps_following_states() {
        let (repo, id) = tiny_repo();
        let mut hl = Highlighter::new(&repo, id).expect("tiny patterns compile");
        let mut buf = TextBuffer::from_str("one\ntwo\nthree");
        let mut cache = SyntaxCache::new(Some(id));
        let _ = cache.ensure(&buf, &mut hl, 2);
        let kept = cache.line_end_states[2];
        let at = buf.offset(LineCol { line: 1, col: 0 });
        let tx = Transaction::single(Edit::insert(at, "\n"));
        cache.on_transaction(&buf, &tx);
        buf.apply(&tx).expect("newline applies");
        assert_eq!(buf.line_count(), 4);
        assert_eq!(cache.line_end_states.len(), 4);
        let range = cache.ensure(&buf, &mut hl, buf.line_count() - 1);
        assert!(range.end - range.start < 4);
        assert_eq!(cache.line_end_states[3], kept);
    }

    #[test]
    fn plain_cache_does_not_highlight() {
        let (repo, id) = tiny_repo();
        let mut hl = Highlighter::new(&repo, id).expect("tiny patterns compile");
        let buf = TextBuffer::from_str("abc\n");
        let mut cache = SyntaxCache::new(None);
        let range = cache.ensure(&buf, &mut hl, 1);
        assert_eq!(range, 0..0);
        assert_eq!(cache.start_state(0), kate_syntax::StateId(0));
    }

    #[test]
    fn merge_regions_counts_two_newlines_on_one_line_once() {
        let buf = TextBuffer::from_str("abcdef");
        let tx = Transaction::new(vec![
            Edit::insert(Offset(1), "\n"),
            Edit::insert(Offset(4), "\n"),
        ])
        .expect("inserts do not overlap");
        let regions = super::regions_of(&buf, &tx);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start, 0);
        assert_eq!(regions[0].old_count, 1);
        assert_eq!(regions[0].new_count, 3);
    }
}
