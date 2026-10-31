// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Line highlighter. The walk matches `AbstractHighlighter::highlightLine`.

use std::sync::Arc;

use fancy_regex::{Regex, RegexInput};

use crate::definition::{ContextId, ContextSwitch, DefId, FoldId, FormatId, Rule, RuleKind};
use crate::regex::RegexCache;
use crate::repository::Repository;
use crate::state::{Frame, State, StateId, StateInterner};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub start: u32,
    pub len: u32,
    pub format: FormatId,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FoldEvent {
    Begin { col: u32, region: FoldId },
    End { col: u32, region: FoldId },
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
    delim: Delimiters,
}

struct Delimiters {
    ascii: [bool; 128],
    other: Vec<char>,
}

struct Prepared<'a> {
    text: &'a str,
    chars: Vec<char>,
    byte_of: Vec<usize>,
}

struct Hit {
    end: usize,
    captures: Vec<String>,
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
            delim: Delimiters::from_chars(&definition.word_delimiters.delimiters),
        }
    }

    pub fn initial_state(&self) -> StateId {
        self.interner.initial()
    }

    /// Deterministic: same (line, start) always yields the same output. Reuses `out` allocations.
    pub fn highlight_line(&mut self, line: &str, start: StateId, out: &mut LineHighlight) {
        out.spans.clear();
        out.folds.clear();
        let mut state = self.interner.get(start).clone();
        let prepared = Prepared::new(line);
        if prepared.chars.is_empty() {
            self.finish_empty(&mut state, out);
        } else {
            self.finish_line(&prepared, &mut state, out);
        }
        out.end_state = self.interner.intern(state);
    }

    pub fn interner(&self) -> &StateInterner {
        &self.interner
    }

    fn finish_empty(&self, state: &mut State, out: &mut LineHighlight) {
        apply_empty_switches(self.repo, state);
        let format = self.context_format(state);
        out.spans.push(Span {
            start: 0,
            len: 0,
            format,
        });
    }

    fn finish_line(&mut self, line: &Prepared<'_>, state: &mut State, out: &mut LineHighlight) {
        let continued = self.paint(line, state, out);
        if !continued {
            apply_end_switches(self.repo, state);
        }
    }

    fn paint(&mut self, line: &Prepared<'_>, state: &mut State, out: &mut LineHighlight) -> bool {
        let mut ink = Ink {
            offset: 0,
            begin: 0,
            current: self.context_format(state),
            continued: false,
            first_non_space: None,
        };
        let mut last_offset = 0;
        let mut stuck = 0_u16;
        loop {
            if no_progress(&mut last_offset, ink.offset, &mut stuck) {
                break;
            }
            if self.consume(line, state, out, &mut ink) {
                continue;
            }
            if ink.offset >= line.chars.len() {
                break;
            }
        }
        if ink.begin < ink.offset {
            push_span(out, ink.begin, line.chars.len() - ink.begin, ink.current);
        }
        ink.continued
    }

    fn consume(
        &mut self,
        line: &Prepared<'_>,
        state: &mut State,
        out: &mut LineHighlight,
        ink: &mut Ink,
    ) -> bool {
        match self.step(line, ink.offset, state, &mut ink.first_non_space, out) {
            Step::Lookahead => true,
            Step::Miss => self.consume_one(state, out, ink),
            Step::Hit {
                end,
                format,
                line_continue,
            } => self.take_hit(line, out, ink, end, format, line_continue),
        }
    }

    fn take_hit(
        &mut self,
        line: &Prepared<'_>,
        out: &mut LineHighlight,
        ink: &mut Ink,
        end: usize,
        format: FormatId,
        line_continue: bool,
    ) -> bool {
        self.note(out, &mut ink.begin, &mut ink.current, ink.offset, format);
        if line_continue && end == line.chars.len() {
            ink.continued = true;
        }
        ink.offset = end;
        false
    }

    fn consume_one(&mut self, state: &mut State, out: &mut LineHighlight, ink: &mut Ink) -> bool {
        if self.fall_through(state) {
            return true;
        }
        let format = self.context_format(state);
        self.note(out, &mut ink.begin, &mut ink.current, ink.offset, format);
        ink.offset += 1;
        false
    }

    fn step(
        &mut self,
        line: &Prepared<'_>,
        offset: usize,
        state: &mut State,
        first_non_space: &mut Option<usize>,
        out: &mut LineHighlight,
    ) -> Step {
        let context = state.top().context;
        let count = self.repo.context(context).rules.len();
        for index in 0..count {
            if !self.rule_can_run(context, index, line, offset, first_non_space) {
                continue;
            }
            let Some(found) = self.match_rule(line, offset, state, context, index) else {
                continue;
            };
            return self.accept(line, offset, state, found, out);
        }
        Step::Miss
    }

    fn rule_can_run(
        &self,
        context: ContextId,
        index: usize,
        line: &Prepared<'_>,
        offset: usize,
        first_non_space: &mut Option<usize>,
    ) -> bool {
        let rule = &self.repo.context(context).rules[index];
        column_allows(rule, offset) && leading_allows(line, rule, offset, first_non_space)
    }

    fn accept(
        &self,
        line: &Prepared<'_>,
        offset: usize,
        state: &mut State,
        found: Found,
        out: &mut LineHighlight,
    ) -> Step {
        let Found {
            context,
            index,
            hit,
        } = found;
        let end = hit.end;
        let captures = arc_captures(hit.captures);
        let rule = &self.repo.context(context).rules[index];
        let switch = rule.switch;
        let lookahead = rule.lookahead;
        let attribute = rule.attribute;
        let line_continue =
            matches!(rule.kind, RuleKind::LineContinue(_)) && end == line.chars.len();
        push_folds(out, offset, rule);
        if lookahead {
            apply_switch(state, switch, captures);
            return Step::Lookahead;
        }
        apply_switch(state, switch, captures);
        let format = attribute.unwrap_or_else(|| self.context_format(state));
        Step::Hit {
            end,
            format,
            line_continue,
        }
    }

    fn match_rule(
        &mut self,
        line: &Prepared<'_>,
        offset: usize,
        state: &State,
        context: ContextId,
        index: usize,
    ) -> Option<Found> {
        let list = keyword_list_of(&self.repo.context(context).rules[index]);
        let words = match list {
            Some(id) => self.repo.keyword_list(id).words.clone(),
            None => Vec::new(),
        };
        let rule = self.repo.context(context).rules[index].clone();
        let top_caps = state.top().captures.clone();
        let caps = top_caps.as_deref().unwrap_or(&[]);
        let mut probe = Probe {
            line,
            offset,
            delim: &self.delim,
            captures: caps,
            words: &words,
            regexes: &mut self.regexes,
            context,
            index,
        };
        let hit = match_kind(&rule, &mut probe)?;
        Some(Found {
            context,
            index,
            hit,
        })
    }

    fn fall_through(&self, state: &mut State) -> bool {
        let Some(switch) = self.repo.context(state.top().context).fallthrough else {
            return false;
        };
        apply_switch(state, switch, None);
        true
    }

    fn note(
        &self,
        out: &mut LineHighlight,
        begin: &mut usize,
        current: &mut FormatId,
        offset: usize,
        format: FormatId,
    ) {
        if format == *current {
            return;
        }
        if offset > 0 {
            push_span(out, *begin, offset - *begin, *current);
        }
        *begin = offset;
        *current = format;
    }

    fn context_format(&self, state: &State) -> FormatId {
        self.repo.context(state.top().context).attribute
    }
}

struct Ink {
    offset: usize,
    begin: usize,
    current: FormatId,
    continued: bool,
    first_non_space: Option<usize>,
}

fn no_progress(last: &mut usize, offset: usize, stuck: &mut u16) -> bool {
    if *last == offset {
        *stuck += 1;
        return *stuck > 1024;
    }
    *last = offset;
    *stuck = 0;
    false
}

enum Step {
    Lookahead,
    Miss,
    Hit {
        end: usize,
        format: FormatId,
        line_continue: bool,
    },
}

struct Found {
    context: ContextId,
    index: usize,
    hit: Hit,
}

struct Probe<'a> {
    line: &'a Prepared<'a>,
    offset: usize,
    delim: &'a Delimiters,
    captures: &'a [String],
    words: &'a [String],
    regexes: &'a mut RegexCache,
    context: ContextId,
    index: usize,
}

fn keyword_list_of(rule: &Rule) -> Option<crate::definition::KeywordListId> {
    match &rule.kind {
        RuleKind::Keyword { list, .. } => Some(*list),
        _ => None,
    }
}

fn match_kind(rule: &Rule, probe: &mut Probe<'_>) -> Option<Hit> {
    let found = match &rule.kind {
        RuleKind::RegExpr { .. } => return match_regex(rule, probe),
        RuleKind::Float
        | RuleKind::Int
        | RuleKind::HlCChar
        | RuleKind::HlCHex
        | RuleKind::HlCOct
        | RuleKind::HlCStringChar => match_number(rule, probe),
        RuleKind::Keyword { .. }
        | RuleKind::LineContinue(_)
        | RuleKind::RangeDetect(_, _)
        | RuleKind::StringDetect { .. }
        | RuleKind::WordDetect { .. } => match_text(rule, probe),
        _ => match_mark(rule, probe),
    };
    found.filter(|hit| hit.end > probe.offset)
}

fn match_mark(rule: &Rule, probe: &Probe<'_>) -> Option<Hit> {
    let line = probe.line;
    let offset = probe.offset;
    match &rule.kind {
        RuleKind::AnyChar(chars) => match_any(line, offset, chars),
        RuleKind::DetectChar(ch) => match_char(line, offset, *ch, rule.dynamic, probe.captures),
        RuleKind::Detect2Chars(first, second) => match_two(line, offset, *first, *second),
        RuleKind::DetectIdentifier => match_ident(line, offset),
        RuleKind::DetectSpaces => match_spaces(line, offset),
        _ => None,
    }
}

fn match_number(rule: &Rule, probe: &Probe<'_>) -> Option<Hit> {
    let line = probe.line;
    let offset = probe.offset;
    match &rule.kind {
        RuleKind::Float => match_float(line, offset, probe.delim),
        RuleKind::Int => match_int(line, offset, probe.delim),
        RuleKind::HlCChar => match_hlc_char(line, offset),
        RuleKind::HlCHex => match_hex(line, offset, probe.delim),
        RuleKind::HlCOct => match_oct(line, offset, probe.delim),
        RuleKind::HlCStringChar => match_string_char(line, offset),
        _ => None,
    }
}

fn match_text(rule: &Rule, probe: &Probe<'_>) -> Option<Hit> {
    let line = probe.line;
    let offset = probe.offset;
    match &rule.kind {
        RuleKind::Keyword { insensitive, .. } => {
            // A failed keyword match skips the rest of the word. Do not retry inside it.
            if offset > 0 && !probe.delim.contains(line.chars[offset - 1]) {
                return None;
            }
            match_keyword(line, offset, probe.delim, probe.words, *insensitive)
        }
        RuleKind::LineContinue(ch) => match_continue(line, offset, *ch),
        RuleKind::RangeDetect(open, close) => match_range(line, offset, *open, *close),
        RuleKind::StringDetect { text, insensitive } => {
            let text = dynamic_text(text, rule.dynamic, probe.captures);
            match_literal(line, offset, &text, *insensitive)
        }
        RuleKind::WordDetect { word, insensitive } => {
            match_word(line, offset, word, *insensitive, probe.delim)
        }
        _ => None,
    }
}

fn apply_empty_switches(repo: &Repository, state: &mut State) {
    let mut guard = 0;
    loop {
        let switch = repo.context(state.top().context).line_empty;
        if matches!(switch, ContextSwitch::Stay) {
            break;
        }
        if !apply_switch(state, switch, None) {
            break;
        }
        if repo
            .context(state.top().context)
            .stop_empty_line_context_switch_loop
        {
            break;
        }
        guard += 1;
        if guard > 1024 {
            break;
        }
    }
}

fn apply_end_switches(repo: &Repository, state: &mut State) {
    let mut guard = 0;
    loop {
        let switch = repo.context(state.top().context).line_end;
        if matches!(switch, ContextSwitch::Stay) {
            break;
        }
        if !apply_switch(state, switch, None) {
            break;
        }
        guard += 1;
        if guard > 1024 {
            break;
        }
    }
}

fn apply_switch(state: &mut State, switch: ContextSwitch, captures: Option<Arc<[String]>>) -> bool {
    match switch {
        ContextSwitch::Stay => true,
        ContextSwitch::Pop(count) => state.pop_frames(count),
        ContextSwitch::Push(context) => {
            state.push_frame(Frame { context, captures });
            true
        }
        ContextSwitch::PopPush(count, context) => {
            let _ = state.pop_frames(count);
            state.push_frame(Frame { context, captures });
            true
        }
    }
}

fn column_allows(rule: &Rule, offset: usize) -> bool {
    match rule.column {
        Some(column) => usize::try_from(column).ok() == Some(offset),
        None => true,
    }
}

fn leading_allows(
    line: &Prepared<'_>,
    rule: &Rule,
    offset: usize,
    cached: &mut Option<usize>,
) -> bool {
    if !rule.first_non_space {
        return true;
    }
    let first = *cached.get_or_insert_with(|| first_non_space(line));
    offset <= first
}

fn first_non_space(line: &Prepared<'_>) -> usize {
    line.chars
        .iter()
        .position(|ch| !qt_is_space(*ch))
        .unwrap_or(line.chars.len())
}

fn push_span(out: &mut LineHighlight, start: usize, len: usize, format: FormatId) {
    out.spans.push(Span {
        start: as_u32(start),
        len: as_u32(len),
        format,
    });
}

fn push_folds(out: &mut LineHighlight, offset: usize, rule: &Rule) {
    let col = as_u32(offset);
    if let (Some(end), Some(_)) = (rule.end_region, rule.begin_region) {
        out.folds.push(FoldEvent::End { col, region: end });
    } else if let Some(region) = rule.end_region {
        out.folds.push(FoldEvent::End { col, region });
    }
    if let Some(region) = rule.begin_region {
        out.folds.push(FoldEvent::Begin { col, region });
    }
}

fn arc_captures(captures: Vec<String>) -> Option<Arc<[String]>> {
    if captures.is_empty() {
        None
    } else {
        Some(Arc::from(captures))
    }
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl Prepared<'_> {
    fn new(text: &str) -> Prepared<'_> {
        let mut chars = Vec::new();
        let mut byte_of = Vec::new();
        for (byte, ch) in text.char_indices() {
            byte_of.push(byte);
            chars.push(ch);
        }
        byte_of.push(text.len());
        Prepared {
            text,
            chars,
            byte_of,
        }
    }

    fn slice(&self, start: usize, end: usize) -> &str {
        &self.text[self.byte_of[start]..self.byte_of[end]]
    }
}

impl Delimiters {
    fn from_chars(chars: &[char]) -> Self {
        let mut ascii = [false; 128];
        let mut other = Vec::new();
        for &ch in chars {
            if (ch as u32) < 128 {
                ascii[ch as usize] = true;
            } else if !other.contains(&ch) {
                other.push(ch);
            }
        }
        Self { ascii, other }
    }

    fn contains(&self, ch: char) -> bool {
        if (ch as u32) < 128 {
            self.ascii[ch as usize]
        } else {
            self.other.contains(&ch)
        }
    }
}

fn at_word_end(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> bool {
    offset == line.chars.len()
        || delim.contains(line.chars[offset])
        || (offset > 0 && delim.contains(line.chars[offset - 1]))
}

fn bounded(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> bool {
    offset == 0 || delim.contains(line.chars[offset - 1])
}

fn match_any(line: &Prepared<'_>, offset: usize, chars: &[char]) -> Option<Hit> {
    if chars.contains(&line.chars[offset]) {
        Some(hit_at(offset + 1))
    } else {
        None
    }
}

fn match_char(
    line: &Prepared<'_>,
    offset: usize,
    ch: char,
    dynamic: bool,
    captures: &[String],
) -> Option<Hit> {
    let expected = if dynamic {
        capture_char(ch, captures)?
    } else {
        ch
    };
    if line.chars[offset] == expected {
        Some(hit_at(offset + 1))
    } else {
        None
    }
}

fn capture_char(ch: char, captures: &[String]) -> Option<char> {
    let digit = ch.to_digit(10)?;
    if digit == 0 {
        return None;
    }
    let index = usize::try_from(digit - 1).unwrap_or(usize::MAX);
    captures.get(index)?.chars().next()
}

fn match_two(line: &Prepared<'_>, offset: usize, first: char, second: char) -> Option<Hit> {
    if offset + 1 < line.chars.len()
        && line.chars[offset] == first
        && line.chars[offset + 1] == second
    {
        Some(hit_at(offset + 2))
    } else {
        None
    }
}

fn match_ident(line: &Prepared<'_>, offset: usize) -> Option<Hit> {
    let ch = line.chars[offset];
    if !ch.is_alphabetic() && ch != '_' {
        return None;
    }
    let mut end = offset + 1;
    while end < line.chars.len() {
        let next = line.chars[end];
        if !next.is_alphanumeric() && next != '_' {
            break;
        }
        end += 1;
    }
    Some(hit_at(end))
}

fn match_spaces(line: &Prepared<'_>, offset: usize) -> Option<Hit> {
    let mut end = offset;
    while end < line.chars.len() && qt_is_space(line.chars[end]) {
        end += 1;
    }
    if end == offset {
        None
    } else {
        Some(hit_at(end))
    }
}

fn match_float(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> Option<Hit> {
    if !bounded(line, offset, delim) {
        return None;
    }
    let mut end = offset;
    while end < line.chars.len() && line.chars[end].is_ascii_digit() {
        end += 1;
    }
    if end >= line.chars.len() || line.chars[end] != '.' {
        return None;
    }
    end += 1;
    while end < line.chars.len() && line.chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == offset + 1 {
        return None;
    }
    Some(hit_at(float_exponent(line, end)))
}

fn float_exponent(line: &Prepared<'_>, end: usize) -> usize {
    let mut exp = end;
    if exp >= line.chars.len() || !matches!(line.chars[exp], 'e' | 'E') {
        return end;
    }
    exp += 1;
    if exp < line.chars.len() && matches!(line.chars[exp], '+' | '-') {
        exp += 1;
    }
    let digits = exp;
    while exp < line.chars.len() && line.chars[exp].is_ascii_digit() {
        exp += 1;
    }
    if exp == digits { end } else { exp }
}

fn match_int(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> Option<Hit> {
    if !bounded(line, offset, delim) {
        return None;
    }
    let mut end = offset;
    while end < line.chars.len() && line.chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == offset {
        None
    } else {
        Some(hit_at(end))
    }
}

fn match_hex(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> Option<Hit> {
    if !bounded(line, offset, delim) || offset + 2 >= line.chars.len() {
        return None;
    }
    if line.chars[offset] != '0' || !matches!(line.chars[offset + 1], 'x' | 'X') {
        return None;
    }
    if !line.chars[offset + 2].is_ascii_hexdigit() {
        return None;
    }
    let mut end = offset + 3;
    while end < line.chars.len() && line.chars[end].is_ascii_hexdigit() {
        end += 1;
    }
    Some(hit_at(end))
}

fn match_oct(line: &Prepared<'_>, offset: usize, delim: &Delimiters) -> Option<Hit> {
    if !bounded(line, offset, delim) || offset + 1 >= line.chars.len() {
        return None;
    }
    if line.chars[offset] != '0' || !is_octal(line.chars[offset + 1]) {
        return None;
    }
    let mut end = offset + 2;
    while end < line.chars.len() && is_octal(line.chars[end]) {
        end += 1;
    }
    Some(hit_at(end))
}

fn is_octal(ch: char) -> bool {
    matches!(ch, '0'..='7')
}

fn match_hlc_char(line: &Prepared<'_>, offset: usize) -> Option<Hit> {
    if offset + 2 >= line.chars.len()
        || line.chars[offset] != '\''
        || line.chars[offset + 1] == '\''
    {
        return None;
    }
    let mut end = escaped_end(line, offset + 1);
    if end == offset + 1 {
        if line.chars[end] == '\\' {
            return None;
        }
        end += 1;
    }
    if end < line.chars.len() && line.chars[end] == '\'' {
        Some(hit_at(end + 1))
    } else {
        None
    }
}

fn match_string_char(line: &Prepared<'_>, offset: usize) -> Option<Hit> {
    let end = escaped_end(line, offset);
    if end == offset {
        None
    } else {
        Some(hit_at(end))
    }
}

fn escaped_end(line: &Prepared<'_>, offset: usize) -> usize {
    if line.chars.get(offset) != Some(&'\\') || offset + 1 >= line.chars.len() {
        return offset;
    }
    match line.chars[offset + 1] {
        'a' | 'b' | 'e' | 'f' | 'n' | 'r' | 't' | 'v' | '"' | '\'' | '?' | '\\' => offset + 2,
        'x' => hex_escape_end(line, offset),
        '0'..='7' => octal_escape_end(line, offset),
        _ => offset,
    }
}

fn hex_escape_end(line: &Prepared<'_>, offset: usize) -> usize {
    if offset + 2 < line.chars.len() && line.chars[offset + 2].is_ascii_hexdigit() {
        if offset + 3 < line.chars.len() && line.chars[offset + 3].is_ascii_hexdigit() {
            return offset + 4;
        }
        return offset + 3;
    }
    offset
}

fn octal_escape_end(line: &Prepared<'_>, offset: usize) -> usize {
    if offset + 2 < line.chars.len() && is_octal(line.chars[offset + 2]) {
        if offset + 3 < line.chars.len() && is_octal(line.chars[offset + 3]) {
            return offset + 4;
        }
        return offset + 3;
    }
    offset + 2
}

fn match_continue(line: &Prepared<'_>, offset: usize, ch: char) -> Option<Hit> {
    if offset + 1 == line.chars.len() && line.chars[offset] == ch {
        Some(hit_at(offset + 1))
    } else {
        None
    }
}

fn match_range(line: &Prepared<'_>, offset: usize, open: char, close: char) -> Option<Hit> {
    if offset + 1 >= line.chars.len() || line.chars[offset] != open {
        return None;
    }
    let mut end = offset + 1;
    while end < line.chars.len() {
        if line.chars[end] == close {
            return Some(hit_at(end + 1));
        }
        end += 1;
    }
    None
}

fn match_keyword(
    line: &Prepared<'_>,
    offset: usize,
    delim: &Delimiters,
    words: &[String],
    insensitive: bool,
) -> Option<Hit> {
    let mut end = offset;
    while end < line.chars.len() && !delim.contains(line.chars[end]) {
        end += 1;
    }
    if end == offset {
        return None;
    }
    let word = line.slice(offset, end);
    if contains_word(words, word, insensitive) {
        Some(hit_at(end))
    } else {
        None
    }
}

fn contains_word(words: &[String], word: &str, insensitive: bool) -> bool {
    if insensitive {
        return words.iter().any(|item| eq_text(item, word, true));
    }
    words
        .binary_search_by(|item| item.as_str().cmp(word))
        .is_ok()
}

fn match_literal(line: &Prepared<'_>, offset: usize, text: &str, insensitive: bool) -> Option<Hit> {
    let len = text.chars().count();
    let end = offset + len;
    if end <= line.chars.len() && eq_text(line.slice(offset, end), text, insensitive) {
        Some(hit_at(end))
    } else {
        None
    }
}

fn dynamic_text(text: &str, dynamic: bool, captures: &[String]) -> String {
    if dynamic {
        replace_plain(text, captures)
    } else {
        text.to_string()
    }
}

fn replace_plain(pattern: &str, captures: &[String]) -> String {
    let mut result = pattern.to_string();
    let last = captures.len().min(9);
    for index in (1..=last).rev() {
        let token = format!("%{index}");
        result = result.replace(&token, &captures[index - 1]);
    }
    result
}

fn match_word(
    line: &Prepared<'_>,
    offset: usize,
    word: &str,
    insensitive: bool,
    delim: &Delimiters,
) -> Option<Hit> {
    let len = word.chars().count();
    if len == 0 || offset + len > line.chars.len() {
        return None;
    }
    if offset > 0 && !delim.contains(line.chars[offset - 1]) && !delim.contains(line.chars[offset])
    {
        return None;
    }
    if !eq_text(line.slice(offset, offset + len), word, insensitive) {
        return None;
    }
    if at_word_end(line, offset + len, delim) {
        Some(hit_at(offset + len))
    } else {
        None
    }
}

fn match_regex(rule: &Rule, probe: &mut Probe<'_>) -> Option<Hit> {
    let RuleKind::RegExpr {
        pattern,
        insensitive,
        minimal,
    } = &rule.kind
    else {
        return None;
    };
    let regex = if rule.dynamic {
        probe
            .regexes
            .dynamic_regex(pattern, *insensitive, *minimal, probe.captures)
            .ok()
            .flatten()?
    } else {
        let index = u16::try_from(probe.index).ok()?;
        probe.regexes.static_regex(probe.context, index)?
    };
    regex_hit(regex, probe.line, probe.offset)
}

fn regex_hit(regex: &Regex, line: &Prepared<'_>, offset: usize) -> Option<Hit> {
    let byte = line.byte_of[offset];
    let input = RegexInput::new(line.text).from_pos(byte).anchored(true);
    let found = regex.captures_input(input).ok().flatten()?;
    let whole = found.get(0)?;
    if whole.start() != byte || whole.end() == byte {
        return None;
    }
    let matched = &line.text[whole.start()..whole.end()];
    let end = offset + matched.chars().count();
    let mut groups = Vec::new();
    let mut last = 0;
    for index in 1..found.len() {
        if found.get(index).is_some() {
            last = index;
        }
    }
    for index in 1..=last {
        groups.push(
            found
                .get(index)
                .map(|item| item.as_str().to_string())
                .unwrap_or_default(),
        );
    }
    Some(Hit {
        end,
        captures: groups,
    })
}

fn hit_at(end: usize) -> Hit {
    Hit {
        end,
        captures: Vec::new(),
    }
}

fn eq_text(left: &str, right: &str, insensitive: bool) -> bool {
    if !insensitive {
        return left == right;
    }
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }
    left.to_lowercase() == right.to_lowercase()
}

fn qt_is_space(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ')
        || matches!(
            ch,
            '\u{00A0}'
                | '\u{1680}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
        )
        || ('\u{2000}'..='\u{200A}').contains(&ch)
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
