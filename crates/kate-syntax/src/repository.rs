// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Definition repository. `add_xml` parses one file. `link` resolves references.

use std::collections::HashMap;

use crate::definition::{
    Context, ContextId, ContextSwitch, DefId, Definition, FoldId, Format, FormatId, KeywordList,
    KeywordListId, Rule, RuleKind, WordDelimiters,
};
use crate::load::{
    ParsedCtx, ParsedDef, ParsedList, PendingItem, PendingKind, PendingRule, parse_xml,
};

const DEFAULT_DELIMITERS: &str = "\t !%&()*+,-./:;<=>?[\\]^{|}~";

pub struct Repository {
    definitions: Vec<Definition>,
    contexts: Vec<Context>,
    formats: Vec<Format>,
    keyword_lists: Vec<KeywordList>,
    fold_names: Vec<String>,
    by_name: HashMap<String, DefId>,
    pending: Vec<Option<PendingCtx>>,
    list_includes: Vec<Option<Vec<String>>>,
    list_owner: Vec<usize>,
    lists_by_def: Vec<HashMap<String, KeywordListId>>,
    formats_by_def: Vec<HashMap<String, FormatId>>,
    contexts_by_def: Vec<HashMap<String, ContextId>>,
}

struct IncludeWrite<'a> {
    rules: &'a mut Vec<Rule>,
    attribute: &'a mut Option<FormatId>,
}

struct PendingCtx {
    def_index: usize,
    line_end: String,
    line_empty: Option<String>,
    fallthrough: Option<String>,
    items: Vec<PendingItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Xml {
        definition: Option<String>,
        line: u64,
        message: String,
    },
    UnknownContext {
        definition: String,
        context: String,
        reference: String,
    },
    UnknownKeywordList {
        definition: String,
        list: String,
    },
    UnknownFormat {
        definition: String,
        format: String,
    },
    DuplicateDefinition {
        name: String,
    },
}

impl Repository {
    pub fn empty() -> Self {
        Self {
            definitions: Vec::new(),
            contexts: Vec::new(),
            formats: Vec::new(),
            keyword_lists: Vec::new(),
            fold_names: Vec::new(),
            by_name: HashMap::new(),
            pending: Vec::new(),
            list_includes: Vec::new(),
            list_owner: Vec::new(),
            lists_by_def: Vec::new(),
            formats_by_def: Vec::new(),
            contexts_by_def: Vec::new(),
        }
    }

    /// Parses one definition. Cross-definition references (`Name##Other`) are recorded and resolved by `link`.
    pub fn add_xml(&mut self, xml: &str) -> Result<DefId, LoadError> {
        self.commit(parse_xml(xml)?)
    }

    /// Resolves IncludeRules, context references and keyword includes. Errors name the definition and context.
    pub fn link(&mut self) -> Result<(), LoadError> {
        self.resolve_keywords()?;
        let pending_ids: Vec<ContextId> = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| ContextId(index as u32)))
            .collect();
        for id in pending_ids {
            self.expand(id, &mut Vec::new())?;
        }
        Ok(())
    }

    pub fn definition(&self, id: DefId) -> &Definition {
        &self.definitions[id.0 as usize]
    }

    pub fn context(&self, id: ContextId) -> &Context {
        &self.contexts[id.0 as usize]
    }

    pub fn format(&self, id: FormatId) -> &Format {
        &self.formats[id.0 as usize]
    }

    pub fn keyword_list(&self, id: KeywordListId) -> &KeywordList {
        &self.keyword_lists[id.0 as usize]
    }

    pub fn by_name(&self, name: &str) -> Option<DefId> {
        self.by_name.get(&name.to_lowercase()).copied()
    }

    /// Highest-priority definition whose extensions glob matches the file name.
    pub fn for_file_name(&self, name: &str) -> Option<DefId> {
        let file = file_name_of(name);
        self.best_match(|def| {
            def.extensions
                .iter()
                .any(|pattern| glob_match(file, pattern))
        })
    }

    pub fn for_mime_type(&self, mime: &str) -> Option<DefId> {
        self.best_match(|def| def.mime_types.iter().any(|item| item == mime))
    }

    fn best_match(&self, pred: impl Fn(&Definition) -> bool) -> Option<DefId> {
        let mut best: Option<(i32, DefId)> = None;
        for (index, def) in self.definitions.iter().enumerate() {
            if !pred(def) {
                continue;
            }
            let id = DefId(u32::try_from(index).unwrap_or(u32::MAX));
            let replace = best
                .map(|(priority, _)| def.priority > priority)
                .unwrap_or(true);
            if replace {
                best = Some((def.priority, id));
            }
        }
        best.map(|(_, id)| id)
    }

    fn commit(&mut self, mut parsed: ParsedDef) -> Result<DefId, LoadError> {
        let key = parsed.name.to_lowercase();
        if self.by_name.contains_key(&key) {
            return Err(LoadError::DuplicateDefinition { name: parsed.name });
        }
        if parsed.contexts.is_empty() {
            return Err(xml_msg(Some(parsed.name), "definition has no contexts"));
        }
        let def_index = self.definitions.len();
        let format_map = plan_formats(self.formats.len(), &parsed)?;
        let context_base = self.contexts.len();
        let contexts = std::mem::take(&mut parsed.contexts);
        let shells = context_shells(&parsed.name, def_index, context_base, &format_map, contexts)?;
        let lists = std::mem::take(&mut parsed.lists);
        if let Some(name) = duplicate_list_name(&lists) {
            return Err(xml_msg(
                Some(parsed.name),
                format!("duplicate keyword list {name}"),
            ));
        }
        self.check_list_room(lists.len(), &parsed.name)?;
        self.push_commit(parsed, def_index, key, format_map, shells, lists)
    }

    fn check_list_room(&self, count: usize, name: &str) -> Result<(), LoadError> {
        if self.keyword_lists.len() + count > usize::from(u16::MAX) {
            return Err(xml_msg(Some(name.to_string()), "too many keyword lists"));
        }
        Ok(())
    }

    fn push_commit(
        &mut self,
        parsed: ParsedDef,
        def_index: usize,
        key: String,
        format_map: HashMap<String, FormatId>,
        shells: Vec<(Context, PendingCtx)>,
        lists: Vec<ParsedList>,
    ) -> Result<DefId, LoadError> {
        let format_start = u16::try_from(self.formats.len()).unwrap_or(u16::MAX);
        self.formats.extend(parsed_formats(&parsed));
        let format_end = u16::try_from(self.formats.len()).unwrap_or(u16::MAX);
        let mut list_map = HashMap::new();
        for list in lists {
            let id = KeywordListId(u16::try_from(self.keyword_lists.len()).unwrap_or(u16::MAX));
            list_map.insert(list.name.clone(), id);
            self.keyword_lists.push(KeywordList {
                name: list.name,
                words: list.words,
            });
            self.list_includes.push(Some(list.includes));
            self.list_owner.push(def_index);
        }
        self.lists_by_def.push(list_map);
        self.formats_by_def.push(format_map);
        let context_start = u32::try_from(self.contexts.len()).unwrap_or(u32::MAX);
        let mut names = HashMap::new();
        for (context, pending) in shells {
            let id = ContextId(u32::try_from(self.contexts.len()).unwrap_or(u32::MAX));
            names.entry(context.name.clone()).or_insert(id);
            self.contexts.push(context);
            self.pending.push(Some(pending));
        }
        let context_end = u32::try_from(self.contexts.len()).unwrap_or(u32::MAX);
        self.contexts_by_def.push(names);
        let id = DefId(u32::try_from(def_index).unwrap_or(u32::MAX));
        self.definitions.push(make_definition(
            &parsed,
            context_start,
            context_end,
            format_start,
            format_end,
        ));
        self.by_name.insert(key, id);
        Ok(id)
    }

    fn resolve_keywords(&mut self) -> Result<(), LoadError> {
        for index in 0..self.keyword_lists.len() {
            self.resolve_list(index, &mut Vec::new())?;
        }
        Ok(())
    }

    fn resolve_list(&mut self, index: usize, stack: &mut Vec<usize>) -> Result<(), LoadError> {
        if stack.contains(&index) {
            return Err(xml_msg(None, "cyclic keyword include"));
        }
        if self
            .list_includes
            .get(index)
            .is_none_or(|slot| slot.is_none())
        {
            return Ok(());
        }
        stack.push(index);
        let includes = self.list_includes[index].take().unwrap_or_default();
        for spec in includes {
            let owner = self.list_owner[index];
            let target = self.lookup_list(owner, &spec)?;
            let target_index = usize::from(target.0);
            if target_index != index {
                self.resolve_list(target_index, stack)?;
                let extra = self.keyword_lists[target_index].words.clone();
                self.keyword_lists[index].words.extend(extra);
            }
        }
        self.keyword_lists[index].words.sort();
        self.keyword_lists[index].words.dedup();
        stack.pop();
        Ok(())
    }

    fn lookup_list(&self, def_index: usize, spec: &str) -> Result<KeywordListId, LoadError> {
        let (name, owner) = match spec.split_once("##") {
            Some((name, def_name)) => {
                let owner = self
                    .find_def(def_name)
                    .ok_or_else(|| unknown_list(self, def_index, spec))?;
                (name, owner)
            }
            None => (spec, def_index),
        };
        self.lists_by_def
            .get(owner)
            .and_then(|map| map.get(name).copied())
            .ok_or_else(|| unknown_list(self, def_index, spec))
    }

    fn expand(&mut self, id: ContextId, stack: &mut Vec<ContextId>) -> Result<(), LoadError> {
        let idx = id.0 as usize;
        if stack.contains(&id) {
            let name = self.contexts[idx].name.clone();
            return Err(xml_msg(None, format!("cyclic IncludeRules in {name}")));
        }
        let Some(pending) = self.pending.get_mut(idx).and_then(Option::take) else {
            return Ok(());
        };
        stack.push(id);
        let built = self.build_context(id, pending, stack);
        stack.pop();
        built
    }

    fn build_context(
        &mut self,
        id: ContextId,
        pending: PendingCtx,
        stack: &mut Vec<ContextId>,
    ) -> Result<(), LoadError> {
        let def_index = pending.def_index;
        let line_end = self.resolve_switch(def_index, &pending.line_end)?;
        let line_empty = self.resolve_line_empty(def_index, &pending.line_empty, line_end)?;
        let fallthrough = self.resolve_fallthrough(def_index, &pending.fallthrough)?;
        let (rules, attribute) = self.build_rules(id, def_index, pending.items, stack)?;
        let ctx = &mut self.contexts[id.0 as usize];
        ctx.line_end = line_end;
        ctx.line_empty = line_empty;
        ctx.fallthrough = fallthrough;
        ctx.rules = rules;
        if let Some(attribute) = attribute {
            ctx.attribute = attribute;
        }
        Ok(())
    }

    fn resolve_line_empty(
        &self,
        def_index: usize,
        raw: &Option<String>,
        line_end: ContextSwitch,
    ) -> Result<ContextSwitch, LoadError> {
        let Some(raw) = raw else {
            return Ok(line_end);
        };
        let switch = self.resolve_switch(def_index, raw)?;
        if matches!(switch, ContextSwitch::Stay) {
            Ok(line_end)
        } else {
            Ok(switch)
        }
    }

    fn resolve_fallthrough(
        &self,
        def_index: usize,
        raw: &Option<String>,
    ) -> Result<Option<ContextSwitch>, LoadError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let switch = self.resolve_switch(def_index, raw)?;
        if matches!(switch, ContextSwitch::Stay) {
            Ok(None)
        } else {
            Ok(Some(switch))
        }
    }

    fn build_rules(
        &mut self,
        id: ContextId,
        def_index: usize,
        items: Vec<PendingItem>,
        stack: &mut Vec<ContextId>,
    ) -> Result<(Vec<Rule>, Option<FormatId>), LoadError> {
        let mut rules = Vec::new();
        let mut attribute = None;
        for item in items {
            match item {
                PendingItem::Rule(rule) => rules.push(self.concrete_rule(def_index, rule)?),
                PendingItem::Include {
                    context,
                    include_attrib,
                } => {
                    let mut out = IncludeWrite {
                        rules: &mut rules,
                        attribute: &mut attribute,
                    };
                    self.inline_include(id, def_index, &context, include_attrib, stack, &mut out)?;
                }
            }
        }
        Ok((rules, attribute))
    }

    fn inline_include(
        &mut self,
        id: ContextId,
        def_index: usize,
        context: &str,
        include_attrib: bool,
        stack: &mut Vec<ContextId>,
        out: &mut IncludeWrite<'_>,
    ) -> Result<(), LoadError> {
        let Some(target) = self.resolve_context_ref(def_index, context)? else {
            return Ok(());
        };
        if target == id {
            return Ok(());
        }
        self.expand(target, stack)?;
        if include_attrib {
            *out.attribute = Some(self.contexts[target.0 as usize].attribute);
        }
        let cloned = self.contexts[target.0 as usize].rules.clone();
        out.rules.extend(cloned);
        Ok(())
    }

    fn concrete_rule(&mut self, def_index: usize, rule: PendingRule) -> Result<Rule, LoadError> {
        let kind = self.concrete_kind(def_index, rule.kind)?;
        let attribute = match rule.attribute {
            None => None,
            Some(name) => Some(self.format_of(def_index, &name)?),
        };
        Ok(Rule {
            kind,
            attribute,
            switch: self.resolve_switch(def_index, &rule.switch_raw)?,
            lookahead: rule.lookahead,
            first_non_space: rule.first_non_space,
            column: rule.column,
            begin_region: self.optional_fold(rule.begin_region)?,
            end_region: self.optional_fold(rule.end_region)?,
            dynamic: rule.dynamic,
        })
    }

    fn concrete_kind(&self, def_index: usize, kind: PendingKind) -> Result<RuleKind, LoadError> {
        match kind {
            PendingKind::Keyword { list, insensitive } => {
                let insensitive =
                    insensitive.unwrap_or(!self.definitions[def_index].case_sensitive_keywords);
                Ok(RuleKind::Keyword {
                    list: self.lookup_list(def_index, &list)?,
                    insensitive,
                })
            }
            PendingKind::RegExpr {
                pattern,
                insensitive,
                minimal,
            } => Ok(RuleKind::RegExpr {
                pattern,
                insensitive,
                minimal,
            }),
            PendingKind::StringDetect { text, insensitive } => {
                Ok(RuleKind::StringDetect { text, insensitive })
            }
            PendingKind::WordDetect { word, insensitive } => {
                Ok(RuleKind::WordDetect { word, insensitive })
            }
            other => Ok(plain_or_char(other)),
        }
    }

    fn format_of(&self, def_index: usize, name: &str) -> Result<FormatId, LoadError> {
        self.formats_by_def
            .get(def_index)
            .and_then(|map| map.get(name).copied())
            .ok_or_else(|| LoadError::UnknownFormat {
                definition: self.definitions[def_index].name.clone(),
                format: name.to_string(),
            })
    }

    fn optional_fold(&mut self, name: Option<String>) -> Result<Option<FoldId>, LoadError> {
        match name {
            None => Ok(None),
            Some(name) => Ok(Some(self.intern_fold(&name)?)),
        }
    }

    fn intern_fold(&mut self, name: &str) -> Result<FoldId, LoadError> {
        if let Some(index) = self.fold_names.iter().position(|existing| existing == name) {
            return Ok(FoldId(u16::try_from(index).unwrap_or(u16::MAX)));
        }
        if self.fold_names.len() > usize::from(u16::MAX) {
            return Err(xml_msg(None, "too many fold regions"));
        }
        let id = FoldId(u16::try_from(self.fold_names.len()).unwrap_or(u16::MAX));
        self.fold_names.push(name.to_string());
        Ok(id)
    }

    fn resolve_switch(&self, def_index: usize, raw: &str) -> Result<ContextSwitch, LoadError> {
        let (pops, target) = split_pops(raw);
        let Some(target) = target else {
            return Ok(pop_or_stay(pops));
        };
        if target.contains('!') {
            return Err(xml_msg(
                Some(self.definitions[def_index].name.clone()),
                "a context switch pushes more than one context",
            ));
        }
        match self.resolve_context_ref(def_index, target)? {
            None => Ok(ContextSwitch::Stay),
            Some(id) if pops == 0 => Ok(ContextSwitch::Push(id)),
            Some(id) => Ok(ContextSwitch::PopPush(pops, id)),
        }
    }

    fn resolve_context_ref(
        &self,
        def_index: usize,
        raw: &str,
    ) -> Result<Option<ContextId>, LoadError> {
        let (ctx_name, def_name) = match raw.split_once("##") {
            Some((ctx, def)) => (ctx, Some(def)),
            None => (raw, None),
        };
        let owner = if let Some(def_name) = def_name {
            match self.find_def(def_name) {
                Some(index) => index,
                None => return Ok(None),
            }
        } else {
            def_index
        };
        if ctx_name.is_empty() {
            return Ok(Some(self.definitions[owner].initial_context));
        }
        match self.find_context(owner, ctx_name) {
            Some(id) => Ok(Some(id)),
            None => Err(LoadError::UnknownContext {
                definition: self.definitions[def_index].name.clone(),
                context: ctx_name.to_string(),
                reference: raw.to_string(),
            }),
        }
    }

    fn find_def(&self, name: &str) -> Option<usize> {
        self.by_name
            .get(&name.to_lowercase())
            .map(|id| id.0 as usize)
    }

    fn find_context(&self, def_index: usize, name: &str) -> Option<ContextId> {
        self.contexts_by_def.get(def_index)?.get(name).copied()
    }
}

fn plain_or_char(kind: PendingKind) -> RuleKind {
    match as_plain(kind) {
        Ok(kind) => kind,
        Err(kind) => as_char(kind),
    }
}

fn as_plain(kind: PendingKind) -> Result<RuleKind, PendingKind> {
    match kind {
        PendingKind::DetectIdentifier => Ok(RuleKind::DetectIdentifier),
        PendingKind::DetectSpaces => Ok(RuleKind::DetectSpaces),
        PendingKind::Float => Ok(RuleKind::Float),
        PendingKind::Int => Ok(RuleKind::Int),
        PendingKind::HlCChar => Ok(RuleKind::HlCChar),
        PendingKind::HlCHex => Ok(RuleKind::HlCHex),
        PendingKind::HlCOct => Ok(RuleKind::HlCOct),
        PendingKind::HlCStringChar => Ok(RuleKind::HlCStringChar),
        other => Err(other),
    }
}

fn as_char(kind: PendingKind) -> RuleKind {
    match kind {
        PendingKind::AnyChar(chars) => RuleKind::AnyChar(chars),
        PendingKind::DetectChar(ch) => RuleKind::DetectChar(ch),
        PendingKind::Detect2Chars(first, second) => RuleKind::Detect2Chars(first, second),
        PendingKind::LineContinue(ch) => RuleKind::LineContinue(ch),
        PendingKind::RangeDetect(open, close) => RuleKind::RangeDetect(open, close),
        other => unreachable!("plain rule fell through to a character rule: {other:?}"),
    }
}

fn duplicate_list_name(lists: &[ParsedList]) -> Option<&str> {
    for (index, list) in lists.iter().enumerate() {
        if lists[..index]
            .iter()
            .any(|earlier| earlier.name == list.name)
        {
            return Some(&list.name);
        }
    }
    None
}

fn parsed_formats(parsed: &ParsedDef) -> Vec<Format> {
    parsed.formats.clone()
}

fn plan_formats(base: usize, parsed: &ParsedDef) -> Result<HashMap<String, FormatId>, LoadError> {
    if base + parsed.formats.len() > usize::from(u16::MAX) {
        return Err(xml_msg(Some(parsed.name.clone()), "too many formats"));
    }
    let mut map = HashMap::new();
    for (offset, format) in parsed.formats.iter().enumerate() {
        let id = FormatId(u16::try_from(base + offset).unwrap_or(u16::MAX));
        if map.insert(format.name.clone(), id).is_some() {
            return Err(LoadError::UnknownFormat {
                definition: parsed.name.clone(),
                format: format.name.clone(),
            });
        }
    }
    Ok(map)
}

fn context_shells(
    def_name: &str,
    def_index: usize,
    base: usize,
    formats: &HashMap<String, FormatId>,
    contexts: Vec<ParsedCtx>,
) -> Result<Vec<(Context, PendingCtx)>, LoadError> {
    if base + contexts.len() > u32::MAX as usize {
        return Err(xml_msg(Some(def_name.to_string()), "too many contexts"));
    }
    let mut shells = Vec::with_capacity(contexts.len());
    for ctx in contexts {
        let attribute =
            formats
                .get(&ctx.attribute)
                .copied()
                .ok_or_else(|| LoadError::UnknownFormat {
                    definition: def_name.to_string(),
                    format: ctx.attribute.clone(),
                })?;
        let context = Context {
            name: ctx.name,
            attribute,
            rules: Vec::new(),
            line_end: ContextSwitch::Stay,
            line_empty: ContextSwitch::Stay,
            fallthrough: None,
            no_indentation_based_folding: ctx.no_indentation_based_folding,
            stop_empty_line_context_switch_loop: ctx.stop_empty_line_context_switch_loop,
        };
        let pending = PendingCtx {
            def_index,
            line_end: ctx.line_end,
            line_empty: ctx.line_empty,
            fallthrough: ctx.fallthrough,
            items: ctx.items,
        };
        shells.push((context, pending));
    }
    Ok(shells)
}

fn make_definition(
    parsed: &ParsedDef,
    context_start: u32,
    context_end: u32,
    format_start: u16,
    format_end: u16,
) -> Definition {
    Definition {
        name: parsed.name.clone(),
        section: parsed.section.clone(),
        version: parsed.version,
        priority: parsed.priority,
        hidden: parsed.hidden,
        extensions: parsed.extensions.clone(),
        mime_types: parsed.mime_types.clone(),
        case_sensitive_keywords: parsed.case_sensitive,
        indenter: parsed.indenter.clone(),
        word_delimiters: word_delimiters(&parsed.additional_delimiters, &parsed.weak_delimiters),
        comments: parsed.comments.clone(),
        indentation_based_folding: parsed.indentation_based_folding,
        initial_context: ContextId(context_start),
        contexts: context_start..context_end,
        formats: format_start..format_end,
        license: parsed.license.clone(),
    }
}

fn word_delimiters(additional: &str, weak: &str) -> WordDelimiters {
    let mut delimiters = unique_chars(DEFAULT_DELIMITERS);
    for ch in additional.chars() {
        if !delimiters.contains(&ch) {
            delimiters.push(ch);
        }
    }
    let weak_chars = unique_chars(weak);
    delimiters.retain(|ch| !weak_chars.contains(ch));
    WordDelimiters {
        delimiters,
        weak: weak_chars,
    }
}

fn unique_chars(value: &str) -> Vec<char> {
    let mut out = Vec::new();
    for ch in value.chars() {
        if !out.contains(&ch) {
            out.push(ch);
        }
    }
    out
}

fn pop_or_stay(pops: u8) -> ContextSwitch {
    if pops == 0 {
        ContextSwitch::Stay
    } else {
        ContextSwitch::Pop(pops)
    }
}

fn split_pops(raw: &str) -> (u8, Option<&str>) {
    let raw = raw.trim();
    if raw.is_empty() || raw == "#stay" {
        return (0, None);
    }
    let mut pops = 0_u8;
    let mut rest = raw;
    while let Some(after) = rest.strip_prefix("#pop") {
        pops = pops.saturating_add(1);
        if let Some(target) = after.strip_prefix('!') {
            let target = target.trim();
            return (
                pops,
                if target.is_empty() {
                    None
                } else {
                    Some(target)
                },
            );
        }
        rest = after;
    }
    if rest.is_empty() {
        (pops, None)
    } else {
        (pops, Some(rest))
    }
}

fn unknown_list(repo: &Repository, def_index: usize, spec: &str) -> LoadError {
    LoadError::UnknownKeywordList {
        definition: repo.definitions[def_index].name.clone(),
        list: spec.to_string(),
    }
}

fn xml_msg(definition: Option<String>, message: impl Into<String>) -> LoadError {
    LoadError::Xml {
        definition,
        line: 0,
        message: message.into(),
    }
}

fn file_name_of(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn glob_match(candidate: &str, wildcard: &str) -> bool {
    let cand: Vec<char> = candidate.chars().collect();
    let wild: Vec<char> = wildcard.chars().collect();
    glob_at(&cand, &wild, isize_end(cand.len()), isize_end(wild.len()))
}

fn isize_end(len: usize) -> isize {
    isize::try_from(len).unwrap_or(isize::MAX) - 1
}

fn glob_at(cand: &[char], wild: &[char], mut cpos: isize, mut wpos: isize) -> bool {
    while wpos >= 0 {
        match wild[wpos as usize] {
            '*' => return glob_star(cand, wild, cpos, wpos),
            '?' => {
                if cpos < 0 {
                    return false;
                }
                cpos -= 1;
                wpos -= 1;
            }
            ch => {
                if cpos < 0 || cand[cpos as usize] != ch {
                    return false;
                }
                cpos -= 1;
                wpos -= 1;
            }
        }
    }
    cpos < 0
}

fn glob_star(cand: &[char], wild: &[char], cpos: isize, wpos: isize) -> bool {
    if cpos < 0 {
        return glob_at(cand, wild, cpos, wpos - 1);
    }
    if wpos == 0 {
        return true;
    }
    let mut eaten = -1;
    while eaten <= cpos {
        if glob_at(cand, wild, eaten, wpos - 1) {
            return true;
        }
        eaten += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{LoadError, Repository};
    use crate::definition::{CommentPosition, CommentStyle, ContextId, ContextSwitch, RuleKind};

    const VENDORED: &[&str] = &[
        include_str!("../definitions/rust.xml"),
        include_str!("../definitions/cpp.xml"),
        include_str!("../definitions/c.xml"),
        include_str!("../definitions/json.xml"),
        include_str!("../definitions/markdown.xml"),
        include_str!("../definitions/cmake.xml"),
    ];

    const INNER: &str = r##"
<language name="Inner" section="Test" version="1" kateversion="5.0" extensions="*.inner" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay">
        <DetectChar char="a" context="#stay" attribute="Normal"/>
        <DetectChar char="b" context="#pop"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    const OUTER: &str = r##"
<language name="Outer" section="Test" version="1" kateversion="5.0" extensions="*.outer" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay">
        <DetectChar char="x" context="#stay"/>
        <IncludeRules context="Normal##Inner"/>
        <DetectChar char="y" context="#stay" attribute="Normal"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    const NOPE: &str = r##"
<language name="Bad" section="Test" version="1" kateversion="5.0" extensions="*.bad" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay">
        <DetectChar char="a" context="Nope"/>
      </context>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    const LOW_CMAKE_EARLY: &str = r##"
<language name="NotCmakeEarly" section="Test" version="1" kateversion="5.0" extensions="CMakeLists.txt" priority="-1" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay"/>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    const LOW_CMAKE_LATE: &str = r##"
<language name="NotCmakeLate" section="Test" version="1" kateversion="5.0" extensions="CMakeLists.txt" priority="-5" license="MIT">
  <highlighting>
    <contexts>
      <context name="Normal" attribute="Normal" lineEndContext="#stay"/>
    </contexts>
    <itemDatas>
      <itemData name="Normal" defStyleNum="dsNormal"/>
    </itemDatas>
  </highlighting>
</language>
"##;

    fn load_vendored() -> Repository {
        let mut repo = Repository::empty();
        for xml in VENDORED {
            repo.add_xml(xml).expect("vendored file parses");
        }
        repo.link().expect("vendored files link");
        repo
    }

    fn assert_switch(switch: ContextSwitch, len: u32) {
        match switch {
            ContextSwitch::Push(id) | ContextSwitch::PopPush(_, id) => {
                assert!(id.0 < len, "push target {id:?} is outside contexts");
            }
            ContextSwitch::Stay | ContextSwitch::Pop(_) => {}
        }
    }

    #[test]
    fn six_vendored_definitions_load_and_link() {
        let repo = load_vendored();
        assert_eq!(repo.definitions.len(), 6);
        let len = repo.contexts.len() as u32;
        assert!(len > 0);
        for context in &repo.contexts {
            assert_switch(context.line_end, len);
            assert_switch(context.line_empty, len);
            if let Some(switch) = context.fallthrough {
                assert_switch(switch, len);
            }
            let _ = repo.format(context.attribute);
            for rule in &context.rules {
                assert_switch(rule.switch, len);
                if let Some(attribute) = rule.attribute {
                    let _ = repo.format(attribute);
                }
                if let RuleKind::Keyword { list, .. } = &rule.kind {
                    let _ = repo.keyword_list(*list);
                }
            }
        }
        let rust = repo.by_name("Rust").expect("rust");
        assert_eq!(repo.definition(rust).license, "MIT");
        let mut saw_rust_ident = false;
        for index in repo.definition(rust).contexts.clone() {
            for rule in &repo.context(ContextId(index)).rules {
                if let RuleKind::RegExpr { pattern, .. } = &rule.kind {
                    assert!(!pattern.contains("&rustIdent;"), "{pattern}");
                    if pattern.contains("a-zA-Z") {
                        saw_rust_ident = true;
                    }
                }
            }
        }
        assert!(saw_rust_ident, "a Rust entity was not expanded");
        for list in &repo.keyword_lists {
            let mut sorted = list.words.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(&list.words, &sorted, "{}", list.name);
        }
        let cmake = repo.by_name("CMake").expect("cmake");
        let cmake_def = repo.definition(cmake);
        assert_eq!(cmake_def.license, "LGPLv2+");
        assert!(cmake_def.word_delimiters.weak.contains(&'.'));
        assert!(!cmake_def.word_delimiters.delimiters.contains(&'.'));
        assert!(repo.definition(rust).comments.iter().any(|style| {
            matches!(
                style,
                CommentStyle::SingleLine { start, position }
                    if start == "//" && *position == CommentPosition::AfterWhitespace
            )
        }));
    }

    #[test]
    fn include_rules_are_inlined_in_order() {
        let mut repo = Repository::empty();
        repo.add_xml(INNER).expect("inner");
        repo.add_xml(OUTER).expect("outer");
        repo.link().expect("link");
        let inner = repo.by_name("Inner").expect("inner id");
        let outer = repo.by_name("Outer").expect("outer id");
        let inner_rules = &repo.context(repo.definition(inner).initial_context).rules;
        let outer_rules = &repo.context(repo.definition(outer).initial_context).rules;
        assert_eq!(outer_rules.len(), 4);
        assert_eq!(outer_rules[0].kind, RuleKind::DetectChar('x'));
        assert_eq!(outer_rules[1], inner_rules[0]);
        assert_eq!(outer_rules[2], inner_rules[1]);
        assert_eq!(outer_rules[3].kind, RuleKind::DetectChar('y'));
    }

    #[test]
    fn unknown_context_reference_is_an_error_naming_it() {
        let mut repo = Repository::empty();
        repo.add_xml(NOPE).expect("parse");
        match repo.link() {
            Err(LoadError::UnknownContext { context, .. }) => assert_eq!(context, "Nope"),
            other => panic!("expected UnknownContext, got {other:?}"),
        }
    }

    #[test]
    fn file_name_matching_prefers_priority() {
        let mut repo = Repository::empty();
        repo.add_xml(LOW_CMAKE_EARLY).expect("early");
        for xml in VENDORED {
            repo.add_xml(xml).expect("vendored file parses");
        }
        repo.add_xml(LOW_CMAKE_LATE).expect("late");
        repo.link().expect("link");
        let cmake = repo.for_file_name("CMakeLists.txt").expect("cmake");
        assert!(repo.definition(cmake).name.eq_ignore_ascii_case("cmake"));
        let rust = repo.for_file_name("main.rs").expect("rust");
        assert!(repo.definition(rust).name.eq_ignore_ascii_case("rust"));
        assert_eq!(repo.for_file_name("x.unknownext"), None);
        let header = repo.for_file_name("foo.h").expect("header");
        assert_eq!(repo.definition(header).name, "C++");
    }
}
