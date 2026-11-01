// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! The owning editor tree.
//!
//! `Workspace` owns the documents, the views, and the layout.
//! `execute` applies one command.
//! It records history, moves every view on that document, and dirties the syntax cache.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use kate_fileio::{
    DecodeOptions, EncodeOptions, Encoding, LineEnding, LoadError, Loaded, load, save,
};
use kate_syntax::{DefId, Repository, Theme};
use kate_text::{
    Bias, CharRange, Edit, EditKind, HistoryEntry, LineCol, Offset, Revision, TextBuffer,
    Transaction, TransactionError,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::command::{Command, CommandError};
use crate::document::{
    Document, DocumentConfig, DocumentId, FileState, Mark, MarkKind, TrailingSpaces,
};
use crate::fanout::{delete_backward, delete_forward, insert_text};
use crate::movement::{
    Extend, MoveContext, Movement, grapheme_after, grapheme_before, move_all, move_selection,
};
use crate::selection::{Selection, Selections};
use crate::syntax_cache::{LineSplice, line_bounds};
use crate::view::{Scroll, View, ViewId};

/// Split direction.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Axis {
    /// Side by side.
    Horizontal,
    /// One above the other.
    Vertical,
}

/// Tab layout for the workspace.
#[derive(Clone, Debug)]
pub enum SplitTree {
    /// One pane of tabs.
    Leaf {
        /// Views in this pane, left to right.
        tabs: Vec<ViewId>,
        /// Index of the visible tab.
        active: usize,
    },
    /// Two panes.
    Split {
        /// Split direction.
        axis: Axis,
        /// Share of space given to `first`.
        ratio: f32,
        /// First pane.
        first: Box<SplitTree>,
        /// Second pane.
        second: Box<SplitTree>,
    },
}

/// Defaults for a new document.
#[derive(Clone, Debug)]
pub struct EditorConfig {
    /// Config copied into each new document.
    pub default_document: DocumentConfig,
    /// Encoding used when the file is not valid UTF-8 and has no BOM.
    pub fallback_encoding: Encoding,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            default_document: DocumentConfig::default(),
            fallback_encoding: DecodeOptions::default().fallback,
        }
    }
}

/// `open` failed.
#[derive(Debug)]
pub enum OpenError {
    /// The path could not be read or decoded.
    Load(kate_fileio::LoadError),
    /// The file looks binary. No document was created.
    ProbablyBinary(PathBuf),
}

/// Result of `close_view`.
#[derive(Debug, PartialEq, Eq)]
pub enum CloseResult {
    /// The view was removed.
    Closed,
    /// The only view of a modified document stayed open.
    RefusedModified(DocumentId),
}

/// Documents, views, and the layout.
pub struct Workspace {
    /// Open documents.
    pub documents: slotmap::SlotMap<DocumentId, Document>,
    /// Open views.
    pub views: slotmap::SlotMap<ViewId, View>,
    /// Tab and split layout.
    pub layout: SplitTree,
    /// View that receives new tabs.
    pub active_view: Option<ViewId>,
    /// Syntax definitions.
    pub syntax: Repository,
    /// Theme for the renderer.
    pub theme: Theme,
    /// Defaults for new documents.
    pub config: EditorConfig,
    untitled: u32,
    extra: HashMap<DocumentId, DocExtra>,
}

struct DocExtra {
    mirror: UndoMirror,
    touched: BTreeSet<u32>,
}

impl Default for DocExtra {
    fn default() -> Self {
        Self {
            mirror: UndoMirror::new(),
            touched: BTreeSet::new(),
        }
    }
}

impl Workspace {
    /// Empty workspace.
    pub fn new(syntax: Repository, theme: Theme, config: EditorConfig) -> Self {
        Self {
            documents: slotmap::SlotMap::with_key(),
            views: slotmap::SlotMap::with_key(),
            layout: SplitTree::empty(),
            active_view: None,
            syntax,
            theme,
            config,
            untitled: 1,
            extra: HashMap::new(),
        }
    }

    /// Create an unsaved document named `Untitled (n)`.
    pub fn new_document(&mut self) -> DocumentId {
        let name = format!("Untitled ({})", self.untitled);
        self.untitled = self.untitled.saturating_add(1);
        let doc = Document::blank(name, self.config.default_document.clone());
        self.insert_doc(doc)
    }

    /// Open `path`. An open document with the same canonical path is reused.
    pub fn open(&mut self, path: &Path) -> Result<DocumentId, OpenError> {
        let canonical = std::fs::canonicalize(path).map_err(load_io)?;
        if let Some(id) = self.document_for_path(&canonical) {
            return Ok(id);
        }
        let loaded = load(&canonical, &self.decode_options()).map_err(OpenError::Load)?;
        if loaded.decoded.probably_binary {
            return Err(OpenError::ProbablyBinary(canonical));
        }
        Ok(self.insert_loaded(canonical, loaded))
    }

    /// Add a view of `doc`. An unknown document returns a null id.
    pub fn new_view(&mut self, doc: DocumentId) -> ViewId {
        if !self.documents.contains_key(doc) {
            return ViewId::default();
        }
        let id = self.views.insert(View {
            document: doc,
            selections: Selections::single(Selection::caret(Offset(0))),
            scroll: Scroll::default(),
            folds: Vec::new(),
            dynamic_wrap: false,
        });
        let near = self.active_view;
        self.layout.insert_tab(id, near);
        self.active_view = Some(id);
        id
    }

    /// Close `view`. The last view of a modified document stays open.
    pub fn close_view(&mut self, view: ViewId) -> CloseResult {
        let Some(state) = self.views.get(view) else {
            return CloseResult::Closed;
        };
        let doc_id = state.document;
        let last = self.views_of(doc_id).len() == 1;
        if last
            && self
                .documents
                .get(doc_id)
                .is_some_and(|doc| doc.history.is_modified())
        {
            return CloseResult::RefusedModified(doc_id);
        }
        self.views.remove(view);
        self.layout.remove_tab(view);
        self.layout.collapse_empty();
        if last {
            self.documents.remove(doc_id);
            self.extra.remove(&doc_id);
        }
        if self.active_view == Some(view) {
            self.active_view = self.layout.first_view();
        }
        CloseResult::Closed
    }

    /// Apply one command to `view`.
    pub fn execute(&mut self, view: ViewId, cmd: Command) -> Result<(), CommandError> {
        if self.views.get(view).is_none() {
            return Err(CommandError::UnknownView(view));
        }
        if run_text(self, view, &cmd)?
            || run_history(self, view, &cmd)?
            || run_motion(self, view, &cmd)?
        {
            return Ok(());
        }
        run_file(self, view, cmd)
    }

    fn insert_doc(&mut self, doc: Document) -> DocumentId {
        let id = self.documents.insert(doc);
        self.extra.insert(id, DocExtra::default());
        id
    }

    fn insert_loaded(&mut self, path: PathBuf, loaded: Loaded) -> DocumentId {
        let name = file_name(&path);
        let definition = self.syntax.for_file_name(&name);
        let text = loaded.decoded.text.to_string();
        let file = FileState {
            path,
            encoding: loaded.decoded.encoding,
            bom: loaded.decoded.bom,
            line_ending: loaded.decoded.line_ending,
            digest_on_disk: loaded.digest,
            mtime_on_disk: loaded.mtime,
            disk_change: None,
        };
        let doc = Document::from_loaded(
            name,
            self.config.default_document.clone(),
            &text,
            file,
            definition,
        );
        self.insert_doc(doc)
    }

    fn decode_options(&self) -> DecodeOptions {
        DecodeOptions {
            fallback: self.config.fallback_encoding,
            ..DecodeOptions::default()
        }
    }

    fn document_for_path(&self, path: &Path) -> Option<DocumentId> {
        self.documents.iter().find_map(|(id, doc)| {
            doc.file
                .as_ref()
                .is_some_and(|file| file.path == path)
                .then_some(id)
        })
    }

    fn doc_id(&self, view: ViewId) -> Result<DocumentId, CommandError> {
        self.views
            .get(view)
            .map(|view| view.document)
            .ok_or(CommandError::UnknownView(view))
    }

    fn views_of(&self, doc: DocumentId) -> Vec<ViewId> {
        self.views
            .iter()
            .filter(|(_, view)| view.document == doc)
            .map(|(id, _)| id)
            .collect()
    }
}

impl SplitTree {
    fn empty() -> Self {
        Self::Leaf {
            tabs: Vec::new(),
            active: 0,
        }
    }

    fn insert_tab(&mut self, view: ViewId, near: Option<ViewId>) {
        if near.is_some_and(|id| self.insert_near(view, id)) {
            return;
        }
        self.insert_first(view);
    }

    fn insert_near(&mut self, view: ViewId, near: ViewId) -> bool {
        match self {
            Self::Leaf { tabs, active } => {
                if !tabs.contains(&near) {
                    return false;
                }
                tabs.push(view);
                *active = tabs.len() - 1;
                true
            }
            Self::Split { first, second, .. } => {
                first.insert_near(view, near) || second.insert_near(view, near)
            }
        }
    }

    fn insert_first(&mut self, view: ViewId) {
        match self {
            Self::Leaf { tabs, active } => {
                tabs.push(view);
                *active = tabs.len() - 1;
            }
            Self::Split { first, .. } => first.insert_first(view),
        }
    }

    fn remove_tab(&mut self, view: ViewId) -> bool {
        match self {
            Self::Leaf { tabs, active } => remove_from_leaf(tabs, active, view),
            Self::Split { first, second, .. } => first.remove_tab(view) || second.remove_tab(view),
        }
    }

    fn collapse_empty(&mut self) {
        while collapse_once(self) {}
    }

    fn first_view(&self) -> Option<ViewId> {
        match self {
            Self::Leaf { tabs, active } => tabs.get(*active).or_else(|| tabs.first()).copied(),
            Self::Split { first, second, .. } => first.first_view().or_else(|| second.first_view()),
        }
    }
}

fn remove_from_leaf(tabs: &mut Vec<ViewId>, active: &mut usize, view: ViewId) -> bool {
    let Some(index) = tabs.iter().position(|id| *id == view) else {
        return false;
    };
    tabs.remove(index);
    if tabs.is_empty() {
        *active = 0;
        return true;
    }
    if *active > index {
        *active -= 1;
    } else if *active >= tabs.len() {
        *active = tabs.len() - 1;
    }
    true
}

fn collapse_once(tree: &mut SplitTree) -> bool {
    if take_sibling(tree, true) || take_sibling(tree, false) {
        return true;
    }
    match tree {
        SplitTree::Leaf { .. } => false,
        SplitTree::Split { first, second, .. } => collapse_once(first) || collapse_once(second),
    }
}

fn take_sibling(tree: &mut SplitTree, drop_first: bool) -> bool {
    let SplitTree::Split { first, second, .. } = tree else {
        return false;
    };
    let empty = if drop_first { first } else { second };
    if !is_empty_leaf(empty) {
        return false;
    }
    let owned = std::mem::replace(tree, SplitTree::empty());
    let SplitTree::Split { first, second, .. } = owned else {
        return false;
    };
    *tree = if drop_first { *second } else { *first };
    true
}

fn is_empty_leaf(tree: &SplitTree) -> bool {
    matches!(tree, SplitTree::Leaf { tabs, .. } if tabs.is_empty())
}

fn run_text(ws: &mut Workspace, view: ViewId, cmd: &Command) -> Result<bool, CommandError> {
    match cmd {
        Command::InsertText(text) => insert_cmd(ws, view, text)?,
        Command::Newline => insert_cmd(ws, view, "\n")?,
        Command::DeleteBackward => delete_cmd(ws, view, false)?,
        Command::DeleteForward => delete_cmd(ws, view, true)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn run_history(ws: &mut Workspace, view: ViewId, cmd: &Command) -> Result<bool, CommandError> {
    match cmd {
        Command::Undo => undo_cmd(ws, view)?,
        Command::Redo => redo_cmd(ws, view)?,
        Command::BeginUndoGroup => group_flag(ws, view, true)?,
        Command::EndUndoGroup => group_flag(ws, view, false)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn run_motion(ws: &mut Workspace, view: ViewId, cmd: &Command) -> Result<bool, CommandError> {
    match cmd {
        Command::Move(movement, extend) => move_cmd(ws, view, *movement, *extend)?,
        Command::SelectAll => select_all(ws, view)?,
        Command::AddCursorBelow => add_cursor(ws, view, Movement::Down)?,
        Command::AddCursorAbove => add_cursor(ws, view, Movement::Up)?,
        Command::RemoveSecondaryCursors => drop_secondary(ws, view)?,
        Command::Scroll(delta) => scroll_by(ws, view, *delta)?,
        Command::ScrollTo(line) => scroll_to(ws, view, *line)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn run_file(ws: &mut Workspace, view: ViewId, cmd: Command) -> Result<(), CommandError> {
    match cmd {
        Command::Save => save_cmd(ws, view),
        Command::SaveAs(path) => save_as_cmd(ws, view, path),
        Command::Reload => reload_cmd(ws, view),
        Command::SetSyntax(id) => set_syntax(ws, view, id),
        Command::ToggleMark(kind, line) => toggle_mark(ws, view, kind, line),
        _ => Ok(()),
    }
}

fn insert_cmd(ws: &mut Workspace, view: ViewId, text: &str) -> Result<(), CommandError> {
    if text.is_empty() {
        return Ok(());
    }
    let doc_id = ws.doc_id(view)?;
    let sel = snapped_selections(ws, view, doc_id)?;
    let (tx, after) = {
        let doc = doc_ref(ws, doc_id, view)?;
        insert_text(&doc.buffer, &sel, text).map_err(CommandError::Transaction)?
    };
    if tx.is_empty() {
        return write_selections(ws, view, after);
    }
    let kind = insert_kind_of(&tx, text);
    apply_change(ws, view, tx, Some(after), kind)
}

fn delete_cmd(ws: &mut Workspace, view: ViewId, forward: bool) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let sel = snapped_selections(ws, view, doc_id)?;
    let (tx, after) = {
        let doc = doc_ref(ws, doc_id, view)?;
        let built = if forward {
            delete_forward(&doc.buffer, &sel)
        } else {
            delete_backward(&doc.buffer, &sel)
        };
        built.map_err(CommandError::Transaction)?
    };
    if tx.is_empty() {
        return write_selections(ws, view, after);
    }
    let kind = if single_pure_delete(&tx) {
        EditKind::Deletion
    } else {
        EditKind::Paste
    };
    apply_change(ws, view, tx, Some(after), kind)
}

fn apply_change(
    ws: &mut Workspace,
    view: ViewId,
    tx: Transaction,
    acting_after: Option<Selections>,
    kind: EditKind,
) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let inverse = checked_invert(&tx).ok_or_else(bad_invert)?;
    let ranges_before = ranges_of(
        &ws.views
            .get(view)
            .ok_or(CommandError::UnknownView(view))?
            .selections,
    );
    let (rev_before, rev_after, splices) = commit_buffer(ws, view, doc_id, &tx)?;
    let ranges_after = match &acting_after {
        Some(after) => ranges_of(after),
        None => {
            let current = &ws
                .views
                .get(view)
                .ok_or(CommandError::UnknownView(view))?
                .selections;
            ranges_of(&current.transformed(&tx))
        }
    };
    record_history(
        ws,
        doc_id,
        HistoryParts {
            tx: tx.clone(),
            inverse,
            kind,
            revision_before: rev_before,
            revision_after: rev_after,
            ranges_before,
            ranges_after,
            splices,
        },
    );
    place_views(ws, doc_id, view, &tx, acting_after);
    Ok(())
}

struct HistoryParts {
    tx: Transaction,
    inverse: Transaction,
    kind: EditKind,
    revision_before: Revision,
    revision_after: Revision,
    ranges_before: Vec<CharRange>,
    ranges_after: Vec<CharRange>,
    splices: Vec<LineSplice>,
}

fn commit_buffer(
    ws: &mut Workspace,
    view: ViewId,
    doc_id: DocumentId,
    tx: &Transaction,
) -> Result<(Revision, Revision, Vec<LineSplice>), CommandError> {
    let doc = doc_mut(ws, doc_id, view)?;
    let rev_before = doc.buffer.revision();
    let (splices, snap) = doc.syntax.plan_and_commit(&doc.buffer, tx);
    match doc.buffer.apply(tx) {
        Ok(rev_after) => {
            refresh_marks(doc);
            Ok((rev_before, rev_after, splices))
        }
        Err(err) => {
            doc.syntax.commit_reverse(&splices);
            doc.syntax.restore_dirty(snap);
            Err(CommandError::Apply(err))
        }
    }
}

fn record_history(ws: &mut Workspace, doc_id: DocumentId, parts: HistoryParts) {
    let splices = parts.splices.clone();
    if let Some(doc) = ws.documents.get_mut(doc_id) {
        doc.history.record(HistoryEntry {
            forward: parts.tx.clone(),
            inverse: parts.inverse,
            kind: parts.kind,
            revision_before: parts.revision_before,
            revision_after: parts.revision_after,
            ranges_before: parts.ranges_before,
            ranges_after: parts.ranges_after,
        });
    }
    let track = ws
        .documents
        .get(doc_id)
        .is_some_and(|doc| doc.config.remove_trailing_spaces == TrailingSpaces::ModifiedLines);
    let extra = ws.extra.entry(doc_id).or_default();
    extra.mirror.record(parts.kind, parts.tx, splices.clone());
    if track {
        note_touched(&mut extra.touched, &splices);
    } else {
        extra.touched.clear();
    }
}

fn place_views(
    ws: &mut Workspace,
    doc_id: DocumentId,
    view: ViewId,
    tx: &Transaction,
    acting_after: Option<Selections>,
) {
    let ids = ws.views_of(doc_id);
    for id in &ids {
        let Some(state) = ws.views.get_mut(*id) else {
            continue;
        };
        if *id == view
            && let Some(after) = &acting_after
        {
            state.selections = after.clone();
            continue;
        }
        state.selections = transform_foreign(&state.selections, tx);
    }
    snap_ids(ws, doc_id, &ids);
}

/// Move selections in a view that did not type the edit.
///
/// Carets and ranges each take one walk over the edits.
/// An empty caret keeps both ends on the right side of inserted text.
/// A range keeps the anchor on the left and the head on the right.
fn transform_foreign(sel: &Selections, tx: &Transaction) -> Selections {
    if sel.iter().all(Selection::is_caret) {
        return collapse_carets(sel.transformed(tx));
    }
    transform_mixed(sel, tx)
}

fn collapse_carets(mut sel: Selections) -> Selections {
    sel.map(|selection| Selection {
        anchor: selection.head,
        head: selection.head,
        goal_col: selection.goal_col,
    });
    sel
}

/// Map a mixed set with one `transformed` call per kind.
fn transform_mixed(sel: &Selections, tx: &Transaction) -> Selections {
    let primary = sel.primary();
    let primary_is_caret = primary.is_caret();
    let mut carets = Vec::new();
    let mut ranges = Vec::new();
    let mut caret_at = None;
    let mut range_at = None;
    for selection in sel.iter().copied() {
        if selection.is_caret() {
            if primary_is_caret && caret_at.is_none() && same_selection(&selection, &primary) {
                caret_at = Some(carets.len());
            }
            carets.push(selection);
            continue;
        }
        if !primary_is_caret && range_at.is_none() && same_selection(&selection, &primary) {
            range_at = Some(ranges.len());
        }
        ranges.push(selection);
    }
    let mapped_ranges = Selections::new(ranges, range_at.unwrap_or(0)).transformed(tx);
    if carets.is_empty() {
        return mapped_ranges;
    }
    let mapped_carets =
        collapse_carets(Selections::new(carets, caret_at.unwrap_or(0)).transformed(tx));
    let mapped_primary = if primary_is_caret {
        mapped_carets.primary()
    } else {
        mapped_ranges.primary()
    };
    let mut items = Vec::with_capacity(mapped_carets.len() + mapped_ranges.len());
    items.extend(mapped_carets.iter().copied());
    items.extend(mapped_ranges.iter().copied());
    let at = items
        .iter()
        .position(|selection| same_selection(selection, &mapped_primary))
        .unwrap_or(0);
    Selections::new(items, at)
}

fn snap_ids(ws: &mut Workspace, doc_id: DocumentId, ids: &[ViewId]) {
    for id in ids {
        let Some(state) = ws.views.get(*id) else {
            continue;
        };
        let mut sel = state.selections.clone();
        let Some(doc) = ws.documents.get(doc_id) else {
            continue;
        };
        snap_set(&doc.buffer, &mut sel);
        if let Some(state) = ws.views.get_mut(*id) {
            state.selections = sel;
        }
    }
}

fn undo_cmd(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    replay(ws, view, true)
}

fn redo_cmd(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    replay(ws, view, false)
}

fn replay(ws: &mut Workspace, view: ViewId, undo: bool) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let ranges = match pop_history(ws, doc_id, undo) {
        None => return Ok(()),
        Some(ranges) => ranges,
    };
    let entry = pop_mirror(ws, doc_id, undo);
    if let Some(entry) = &entry {
        apply_mirror_syntax(ws, doc_id, entry, undo);
    } else if let Some(doc) = ws.documents.get_mut(doc_id) {
        doc.syntax.invalidate_all();
    }
    seal_mirror(ws, doc_id);
    refresh_doc_marks(ws, doc_id);
    move_views_for_replay(ws, doc_id, view, entry.as_ref(), undo, &ranges);
    Ok(())
}

fn pop_history(ws: &mut Workspace, doc_id: DocumentId, undo: bool) -> Option<Vec<CharRange>> {
    let doc = ws.documents.get_mut(doc_id)?;
    if undo {
        doc.history.undo(&mut doc.buffer)
    } else {
        doc.history.redo(&mut doc.buffer)
    }
}

fn pop_mirror(ws: &mut Workspace, doc_id: DocumentId, undo: bool) -> Option<HistEntry> {
    let extra = ws.extra.entry(doc_id).or_default();
    if undo {
        extra.mirror.take_undo()
    } else {
        extra.mirror.take_redo()
    }
}

fn apply_mirror_syntax(ws: &mut Workspace, doc_id: DocumentId, entry: &HistEntry, undo: bool) {
    let track = ws
        .documents
        .get(doc_id)
        .is_some_and(|doc| doc.config.remove_trailing_spaces == TrailingSpaces::ModifiedLines);
    if let Some(doc) = ws.documents.get_mut(doc_id) {
        if undo {
            for part in entry.parts.iter().rev() {
                doc.syntax.commit_reverse(&part.splices);
            }
        } else {
            for part in &entry.parts {
                doc.syntax.commit_forward(&part.splices);
            }
        }
    }
    if !track {
        if let Some(extra) = ws.extra.get_mut(&doc_id) {
            extra.touched.clear();
        }
        return;
    }
    if let Some(extra) = ws.extra.get_mut(&doc_id) {
        if undo {
            note_touched_reverse(&mut extra.touched, entry);
        } else {
            for part in &entry.parts {
                note_touched(&mut extra.touched, &part.splices);
            }
        }
    }
}

fn seal_mirror(ws: &mut Workspace, doc_id: DocumentId) {
    let clean = ws
        .documents
        .get(doc_id)
        .is_some_and(|doc| !doc.history.is_modified());
    ws.extra
        .entry(doc_id)
        .or_default()
        .mirror
        .seal_if_clean(clean);
}

fn refresh_doc_marks(ws: &mut Workspace, doc_id: DocumentId) {
    if let Some(doc) = ws.documents.get_mut(doc_id) {
        refresh_marks(doc);
    }
}

fn move_views_for_replay(
    ws: &mut Workspace,
    doc_id: DocumentId,
    view: ViewId,
    entry: Option<&HistEntry>,
    undo: bool,
    ranges: &[CharRange],
) {
    let ids = ws.views_of(doc_id);
    if let Some(entry) = entry {
        let parts: Vec<&Transaction> = if undo {
            entry.parts.iter().rev().map(|part| &part.tx).collect()
        } else {
            entry.parts.iter().map(|part| &part.tx).collect()
        };
        for tx in parts {
            let mapped = if undo {
                checked_invert(tx)
            } else {
                Some(tx.clone())
            };
            let Some(mapped) = mapped else {
                continue;
            };
            shift_ids(ws, &ids, &mapped);
        }
    }
    if let Some(state) = ws.views.get_mut(view) {
        state.selections = selections_from_ranges(ranges);
    }
    snap_ids(ws, doc_id, &ids);
}

fn shift_ids(ws: &mut Workspace, ids: &[ViewId], tx: &Transaction) {
    for id in ids {
        if let Some(state) = ws.views.get_mut(*id) {
            state.selections = transform_foreign(&state.selections, tx);
        }
    }
}

fn group_flag(ws: &mut Workspace, view: ViewId, begin: bool) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    {
        let doc = doc_mut(ws, doc_id, view)?;
        if begin {
            doc.history.begin_group();
        } else {
            doc.history.end_group();
        }
    }
    let mirror = &mut ws.extra.entry(doc_id).or_default().mirror;
    if begin {
        mirror.begin_group();
    } else {
        mirror.end_group();
    }
    Ok(())
}

fn move_cmd(
    ws: &mut Workspace,
    view: ViewId,
    movement: Movement,
    extend: Extend,
) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let tab = doc_ref(ws, doc_id, view)?.config.tab_width;
    let mut sel = ws
        .views
        .get(view)
        .ok_or(CommandError::UnknownView(view))?
        .selections
        .clone();
    {
        let doc = doc_ref(ws, doc_id, view)?;
        move_all(
            &doc.buffer,
            &mut sel,
            movement,
            extend,
            &MoveContext { tab_width: tab },
        );
    }
    write_selections(ws, view, sel)
}

fn select_all(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let len = doc_ref(ws, doc_id, view)?.buffer.len();
    let sel = Selections::single(Selection {
        anchor: Offset(0),
        head: len,
        goal_col: None,
    });
    write_selections(ws, view, sel)
}

fn add_cursor(ws: &mut Workspace, view: ViewId, movement: Movement) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let tab = doc_ref(ws, doc_id, view)?.config.tab_width;
    let existing = ws
        .views
        .get(view)
        .ok_or(CommandError::UnknownView(view))?
        .selections
        .clone();
    let primary = existing.primary();
    let added = {
        let doc = doc_ref(ws, doc_id, view)?;
        let ctx = MoveContext { tab_width: tab };
        cursors_on_other_lines(&doc.buffer, &existing, movement, &ctx)
    };
    if added.is_empty() {
        return Ok(());
    }
    let mut items: Vec<Selection> = existing.iter().copied().collect();
    let primary_at = items
        .iter()
        .position(|item| same_selection(item, &primary))
        .unwrap_or(0);
    items.extend(added);
    write_selections(ws, view, Selections::new(items, primary_at))
}

fn cursors_on_other_lines(
    buf: &TextBuffer,
    existing: &Selections,
    movement: Movement,
    ctx: &MoveContext,
) -> Vec<Selection> {
    let mut added = Vec::new();
    for selection in existing.iter() {
        let moved = move_selection(buf, *selection, movement, Extend::No, ctx);
        if buf.line_col(moved.head).line == buf.line_col(selection.head).line {
            continue;
        }
        added.push(Selection {
            anchor: moved.head,
            head: moved.head,
            goal_col: moved.goal_col,
        });
    }
    added
}

fn drop_secondary(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    let Some(state) = ws.views.get_mut(view) else {
        return Err(CommandError::UnknownView(view));
    };
    state.selections.remove_secondary();
    Ok(())
}

fn scroll_by(ws: &mut Workspace, view: ViewId, delta: i32) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let last = doc_ref(ws, doc_id, view)?
        .buffer
        .line_count()
        .saturating_sub(1);
    let Some(state) = ws.views.get_mut(view) else {
        return Err(CommandError::UnknownView(view));
    };
    let next = i64::from(state.scroll.first_line) + i64::from(delta);
    state.scroll.first_line = clamp_i64(next, last);
    Ok(())
}

fn scroll_to(ws: &mut Workspace, view: ViewId, line: u32) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let last = doc_ref(ws, doc_id, view)?
        .buffer
        .line_count()
        .saturating_sub(1);
    let Some(state) = ws.views.get_mut(view) else {
        return Err(CommandError::UnknownView(view));
    };
    state.scroll.first_line = line.min(last);
    Ok(())
}

fn save_cmd(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let path = doc_ref(ws, doc_id, view)?
        .file
        .as_ref()
        .map(|file| file.path.clone())
        .ok_or(CommandError::NoFile)?;
    write_file(ws, view, doc_id, &path, false)
}

fn save_as_cmd(ws: &mut Workspace, view: ViewId, path: PathBuf) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    write_file(ws, view, doc_id, &path, true)
}

fn write_file(
    ws: &mut Workspace,
    view: ViewId,
    doc_id: DocumentId,
    path: &Path,
    replace_path: bool,
) -> Result<(), CommandError> {
    prepare_save(ws, view, doc_id)?;
    let saved = {
        let doc = doc_ref(ws, doc_id, view)?;
        let opts = encode_options(doc, &ws.config);
        save(path, doc.buffer.text(), &opts).map_err(CommandError::Save)?
    };
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = file_name(&canonical);
    let definition = ws.syntax.for_file_name(&name);
    let (encoding, bom, line_ending) = file_encoding(doc_ref(ws, doc_id, view)?, &ws.config);
    let doc = doc_mut(ws, doc_id, view)?;
    doc.file = Some(FileState {
        path: canonical,
        encoding,
        bom,
        line_ending,
        digest_on_disk: saved.digest,
        mtime_on_disk: saved.mtime,
        disk_change: None,
    });
    if replace_path {
        doc.name = name;
        doc.syntax.set_definition(definition);
    }
    doc.history.mark_saved();
    let mirror = &mut ws.extra.entry(doc_id).or_default();
    mirror.mirror.mark_saved();
    mirror.touched.clear();
    Ok(())
}

fn prepare_save(ws: &mut Workspace, view: ViewId, doc_id: DocumentId) -> Result<(), CommandError> {
    let edits = {
        let doc = doc_ref(ws, doc_id, view)?;
        let touched = ws.extra.get(&doc_id).map(|extra| &extra.touched);
        cleanup_edits(&doc.buffer, &doc.config, touched)
    };
    if edits.is_empty() {
        return Ok(());
    }
    let tx = Transaction::new(edits).map_err(CommandError::Transaction)?;
    apply_change(ws, view, tx, None, EditKind::Programmatic)
}

fn reload_cmd(ws: &mut Workspace, view: ViewId) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let path = doc_ref(ws, doc_id, view)?
        .file
        .as_ref()
        .map(|file| file.path.clone())
        .ok_or(CommandError::NoFile)?;
    let loaded = load(&path, &ws.decode_options()).map_err(CommandError::Load)?;
    if loaded.decoded.probably_binary {
        return Err(CommandError::Load(LoadError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "probably binary",
        ))));
    }
    replace_from_load(ws, view, doc_id, loaded)
}

fn replace_from_load(
    ws: &mut Workspace,
    view: ViewId,
    doc_id: DocumentId,
    loaded: Loaded,
) -> Result<(), CommandError> {
    let new_text = loaded.decoded.text.to_string();
    {
        let doc = doc_mut(ws, doc_id, view)?;
        let old = doc.buffer.text().to_string();
        if old != new_text {
            let tx = Transaction::single(Edit {
                start: Offset(0),
                old,
                new: new_text,
            });
            doc.buffer.apply(&tx).map_err(CommandError::Apply)?;
        }
        if let Some(file) = doc.file.as_mut() {
            file.encoding = loaded.decoded.encoding;
            file.bom = loaded.decoded.bom;
            file.line_ending = loaded.decoded.line_ending;
            file.digest_on_disk = loaded.digest;
            file.mtime_on_disk = loaded.mtime;
            file.disk_change = None;
        }
        doc.history.clear();
        doc.syntax.invalidate_all();
        doc.syntax.fit(doc.buffer.line_count());
        refresh_marks(doc);
    }
    ws.extra.insert(doc_id, DocExtra::default());
    let ids = ws.views_of(doc_id);
    clamp_ids(ws, doc_id, &ids);
    Ok(())
}

fn clamp_ids(ws: &mut Workspace, doc_id: DocumentId, ids: &[ViewId]) {
    for id in ids {
        let Some(state) = ws.views.get(*id) else {
            continue;
        };
        let mut sel = state.selections.clone();
        let Some(doc) = ws.documents.get(doc_id) else {
            continue;
        };
        let len = doc.buffer.len().0;
        sel.map(|selection| Selection {
            anchor: Offset(selection.anchor.0.min(len)),
            head: Offset(selection.head.0.min(len)),
            goal_col: selection.goal_col,
        });
        snap_set(&doc.buffer, &mut sel);
        if let Some(state) = ws.views.get_mut(*id) {
            state.selections = sel;
        }
    }
}

fn set_syntax(ws: &mut Workspace, view: ViewId, id: Option<DefId>) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    doc_mut(ws, doc_id, view)?.syntax.set_definition(id);
    Ok(())
}

fn toggle_mark(
    ws: &mut Workspace,
    view: ViewId,
    kind: MarkKind,
    line: u32,
) -> Result<(), CommandError> {
    let doc_id = ws.doc_id(view)?;
    let doc = doc_mut(ws, doc_id, view)?;
    let last = doc.buffer.line_count().saturating_sub(1);
    let line = line.min(last);
    if let Some(index) = mark_index(doc, kind, line) {
        doc.marks.remove(index);
        return Ok(());
    }
    let offset = doc.buffer.offset(LineCol { line, col: 0 });
    doc.marks.push(Mark {
        anchor: doc.buffer.anchor(offset, Bias::Left),
        kind,
    });
    Ok(())
}

fn mark_index(doc: &Document, kind: MarkKind, line: u32) -> Option<usize> {
    doc.marks
        .iter()
        .position(|mark| mark.kind == kind && mark_line(&doc.buffer, mark.anchor) == Some(line))
}

fn mark_line(buf: &TextBuffer, anchor: kate_text::Anchor) -> Option<u32> {
    let offset = if anchor.revision == buf.revision() {
        anchor.offset
    } else {
        buf.resolve(anchor).ok()?
    };
    Some(buf.line_col(offset).line)
}

fn refresh_marks(doc: &mut Document) {
    let marks = std::mem::take(&mut doc.marks);
    let mut kept = Vec::with_capacity(marks.len());
    for mut mark in marks {
        if let Ok(anchor) = doc.buffer.refresh(mark.anchor) {
            mark.anchor = anchor;
            kept.push(mark);
        }
    }
    doc.marks = kept;
}

fn doc_ref(ws: &Workspace, doc_id: DocumentId, view: ViewId) -> Result<&Document, CommandError> {
    ws.documents
        .get(doc_id)
        .ok_or(CommandError::UnknownView(view))
}

fn doc_mut(
    ws: &mut Workspace,
    doc_id: DocumentId,
    view: ViewId,
) -> Result<&mut Document, CommandError> {
    ws.documents
        .get_mut(doc_id)
        .ok_or(CommandError::UnknownView(view))
}

fn write_selections(
    ws: &mut Workspace,
    view: ViewId,
    selections: Selections,
) -> Result<(), CommandError> {
    let Some(state) = ws.views.get_mut(view) else {
        return Err(CommandError::UnknownView(view));
    };
    state.selections = selections;
    Ok(())
}

fn snapped_selections(
    ws: &mut Workspace,
    view: ViewId,
    doc_id: DocumentId,
) -> Result<Selections, CommandError> {
    let mut sel = ws
        .views
        .get(view)
        .ok_or(CommandError::UnknownView(view))?
        .selections
        .clone();
    snap_set(&doc_ref(ws, doc_id, view)?.buffer, &mut sel);
    write_selections(ws, view, sel.clone())?;
    Ok(sel)
}

fn file_encoding(doc: &Document, config: &EditorConfig) -> (Encoding, bool, LineEnding) {
    match &doc.file {
        Some(file) => (file.encoding, file.bom, file.line_ending),
        None => (config.fallback_encoding, false, LineEnding::Lf),
    }
}

fn encode_options(doc: &Document, config: &EditorConfig) -> EncodeOptions {
    let (encoding, bom, line_ending) = file_encoding(doc, config);
    EncodeOptions {
        encoding,
        bom,
        line_ending,
        ensure_trailing_newline: false,
    }
}

fn cleanup_edits(
    buf: &TextBuffer,
    config: &DocumentConfig,
    touched: Option<&BTreeSet<u32>>,
) -> Vec<Edit> {
    let mut edits = Vec::new();
    match config.remove_trailing_spaces {
        TrailingSpaces::Never => {}
        TrailingSpaces::All => {
            for line in 0..buf.line_count() {
                push_strip(buf, line, &mut edits);
            }
        }
        TrailingSpaces::ModifiedLines => {
            if let Some(lines) = touched {
                for line in lines {
                    if *line < buf.line_count() {
                        push_strip(buf, *line, &mut edits);
                    }
                }
            }
        }
    }
    if config.ensure_trailing_newline {
        let strip_last = strips_line(config, touched, buf.line_count().saturating_sub(1));
        push_final_newline(buf, strip_last, &mut edits);
    }
    edits
}

fn strips_line(config: &DocumentConfig, touched: Option<&BTreeSet<u32>>, line: u32) -> bool {
    match config.remove_trailing_spaces {
        TrailingSpaces::Never => false,
        TrailingSpaces::All => true,
        TrailingSpaces::ModifiedLines => touched.is_some_and(|lines| lines.contains(&line)),
    }
}

fn push_strip(buf: &TextBuffer, line: u32, edits: &mut Vec<Edit>) {
    let (start, end) = line_bounds(buf, line);
    let cut = trim_trailing(buf, start, end);
    if cut == end {
        return;
    }
    let old = buf.text().slice(cut..end).to_string();
    edits.push(Edit::delete(Offset(cut), old));
}

fn trim_trailing(buf: &TextBuffer, start: usize, end: usize) -> usize {
    let mut cut = end;
    let rope = buf.text();
    while cut > start {
        let ch = rope.char(cut - 1);
        if ch != ' ' && ch != '\t' {
            break;
        }
        cut -= 1;
    }
    cut
}

/// Add a newline when the last line still has text after the strip.
///
/// Kate checks the last line after the strip. An empty last line stays empty.
/// See `DocumentPrivate::removeTrailingSpacesAndAddNewLineAtEof` in
/// `katedocument.cpp` lines 5750-5754, KTextEditor v6.31.0.
fn push_final_newline(buf: &TextBuffer, strip_last: bool, edits: &mut Vec<Edit>) {
    let last = buf.line_count().saturating_sub(1);
    let (start, end) = line_bounds(buf, last);
    let cut = if strip_last {
        trim_trailing(buf, start, end)
    } else {
        end
    };
    if cut == start {
        return;
    }
    edits.push(Edit::insert(Offset(cut), "\n"));
}

fn note_touched(touched: &mut BTreeSet<u32>, splices: &[LineSplice]) {
    if splices.is_empty() {
        return;
    }
    let old = std::mem::take(touched);
    *touched = remap_forward(&old, splices);
    for splice in splices {
        for offset in 0..splice.new_len {
            touched.insert(splice.post_at.saturating_add(offset));
        }
    }
}

fn note_touched_reverse(touched: &mut BTreeSet<u32>, entry: &HistEntry) {
    for part in entry.parts.iter().rev() {
        let old = std::mem::take(touched);
        *touched = remap_reverse(&old, &part.splices);
        for splice in &part.splices {
            let count = u32::try_from(splice.old_states.len()).unwrap_or(u32::MAX);
            for offset in 0..count {
                touched.insert(splice.pre_at.saturating_add(offset));
            }
        }
    }
}

/// Walk sorted lines and sorted splices once.
fn remap_forward(lines: &BTreeSet<u32>, splices: &[LineSplice]) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    let mut index = 0usize;
    let mut delta = 0i32;
    for line in lines {
        while index < splices.len() && splices[index].pre_at <= *line {
            let old_len = splices[index].old_states.len() as u32;
            let old_end = splices[index].pre_at.saturating_add(old_len);
            if *line < old_end {
                break;
            }
            let change = splices[index].new_len as i32 - old_len as i32;
            delta = delta.saturating_add(change);
            index += 1;
        }
        if let Some(splice) = splices.get(index) {
            let old_end = splice.pre_at.saturating_add(splice.old_states.len() as u32);
            if splice.pre_at <= *line && *line < old_end {
                out.insert(splice.post_at);
                continue;
            }
        }
        out.insert(add_delta(*line, delta));
    }
    out
}

fn remap_reverse(lines: &BTreeSet<u32>, splices: &[LineSplice]) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    let mut index = 0usize;
    let mut delta = 0i32;
    for line in lines {
        while index < splices.len() && splices[index].post_at <= *line {
            let post_end = splices[index]
                .post_at
                .saturating_add(splices[index].new_len);
            if *line < post_end {
                break;
            }
            let change = splices[index].new_len as i32 - splices[index].old_states.len() as i32;
            delta = delta.saturating_add(change);
            index += 1;
        }
        if let Some(splice) = splices.get(index) {
            let post_end = splice.post_at.saturating_add(splice.new_len);
            if splice.post_at <= *line && *line < post_end {
                out.insert(splice.pre_at);
                continue;
            }
        }
        out.insert(add_delta(*line, delta.saturating_neg()));
    }
    out
}

fn add_delta(line: u32, delta: i32) -> u32 {
    let next = i64::from(line) + i64::from(delta);
    u32::try_from(next.max(0)).unwrap_or(u32::MAX)
}

fn load_io(err: std::io::Error) -> OpenError {
    OpenError::Load(LoadError::Io(err))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn clamp_i64(line: i64, last: u32) -> u32 {
    let max = i64::from(last);
    let clamped = line.clamp(0, max);
    u32::try_from(clamped).unwrap_or(0)
}

fn ranges_of(sel: &Selections) -> Vec<CharRange> {
    let primary = sel.primary();
    let mut out = Vec::with_capacity(sel.len());
    out.push(primary.range());
    for item in sel.iter() {
        if same_selection(item, &primary) {
            continue;
        }
        out.push(item.range());
    }
    out
}

fn same_selection(left: &Selection, right: &Selection) -> bool {
    left.anchor == right.anchor && left.head == right.head && left.goal_col == right.goal_col
}

fn selections_from_ranges(ranges: &[CharRange]) -> Selections {
    if ranges.is_empty() {
        return Selections::single(Selection::caret(Offset(0)));
    }
    let items = ranges
        .iter()
        .map(|range| Selection {
            anchor: range.start(),
            head: range.end(),
            goal_col: None,
        })
        .collect();
    Selections::new(items, 0)
}

fn snap_set(buf: &TextBuffer, sel: &mut Selections) {
    sel.map(|selection| snap_one(buf, selection));
}

fn snap_one(buf: &TextBuffer, selection: Selection) -> Selection {
    let len = buf.len().0;
    let anchor = Offset(selection.anchor.0.min(len));
    let head = Offset(selection.head.0.min(len));
    if anchor == head {
        let at = floor_boundary(buf, anchor);
        return Selection {
            anchor: at,
            head: at,
            goal_col: selection.goal_col,
        };
    }
    let min = floor_boundary(buf, anchor.min(head));
    let max = ceil_boundary(buf, anchor.max(head));
    if anchor <= head {
        Selection {
            anchor: min,
            head: max,
            goal_col: selection.goal_col,
        }
    } else {
        Selection {
            anchor: max,
            head: min,
            goal_col: selection.goal_col,
        }
    }
}

fn floor_boundary(buf: &TextBuffer, offset: Offset) -> Offset {
    let at = Offset(offset.0.min(buf.len().0));
    if at.0 == 0 || ascii_boundary(buf, at) {
        return at;
    }
    let prev = grapheme_before(buf, at);
    if grapheme_after(buf, prev) == at {
        at
    } else {
        prev
    }
}

fn ceil_boundary(buf: &TextBuffer, offset: Offset) -> Offset {
    let at = Offset(offset.0.min(buf.len().0));
    if ascii_boundary(buf, at) {
        return at;
    }
    let prev = grapheme_before(buf, at);
    if grapheme_after(buf, prev) == at {
        at
    } else {
        grapheme_after(buf, at)
    }
}

fn ascii_boundary(buf: &TextBuffer, offset: Offset) -> bool {
    let rope = buf.text();
    let at = offset.0.min(rope.len_chars());
    if at == 0 || at == rope.len_chars() {
        return true;
    }
    let prev = rope.char(at - 1);
    let next = rope.char(at);
    prev.is_ascii() && next.is_ascii() && prev != '\r' && next != '\n'
}

fn one_cluster(text: &str) -> bool {
    let mut clusters = text.graphemes(true);
    clusters.next().is_some() && clusters.next().is_none()
}

fn single_edit(tx: &Transaction) -> Option<&Edit> {
    let edits = tx.edits();
    if edits.len() == 1 {
        Some(&edits[0])
    } else {
        None
    }
}

fn single_pure_delete(tx: &Transaction) -> bool {
    single_edit(tx).is_some_and(|edit| edit.new.is_empty() && !edit.old.is_empty())
}

fn pure_insert(tx: &Transaction) -> Option<&Edit> {
    let edit = single_edit(tx)?;
    if edit.old.is_empty() && !edit.new.is_empty() {
        Some(edit)
    } else {
        None
    }
}

fn pure_delete(tx: &Transaction) -> Option<&Edit> {
    if single_pure_delete(tx) {
        single_edit(tx)
    } else {
        None
    }
}

fn scalar_len(text: &str) -> usize {
    text.chars().count()
}

fn checked_invert(tx: &Transaction) -> Option<Transaction> {
    let mut shift = 0isize;
    let mut edits = Vec::with_capacity(tx.edits().len());
    for edit in tx.edits() {
        let start_i = isize::try_from(edit.start.0).ok()?;
        let post = start_i.checked_add(shift)?;
        let start = usize::try_from(post).ok()?;
        edits.push(Edit {
            start: Offset(start),
            old: edit.new.clone(),
            new: edit.old.clone(),
        });
        shift = shift.checked_add(edit.delta())?;
    }
    Transaction::new(edits).ok()
}

fn bad_invert() -> CommandError {
    CommandError::Transaction(TransactionError::Overlap {
        first: 0,
        second: 0,
    })
}

#[derive(Clone, Debug)]
struct HistEntry {
    kind: EditKind,
    parts: Vec<HistPart>,
    typing_end: Option<usize>,
    deletion_edge: Option<usize>,
}

#[derive(Clone, Debug)]
struct HistPart {
    tx: Transaction,
    splices: Vec<LineSplice>,
}

struct UndoMirror {
    undo: Vec<HistEntry>,
    redo: Vec<HistEntry>,
    group_open: bool,
    coalesce_break: bool,
}

impl UndoMirror {
    fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            group_open: false,
            coalesce_break: false,
        }
    }

    fn record(&mut self, kind: EditKind, tx: Transaction, splices: Vec<LineSplice>) {
        self.redo.clear();
        let part = HistPart { tx, splices };
        if self.will_merge(kind, &part.tx) {
            self.append(part);
            return;
        }
        self.push_new(kind, part);
    }

    fn will_merge(&self, kind: EditKind, tx: &Transaction) -> bool {
        let Some(last) = self.undo.last() else {
            return false;
        };
        if self.coalesce_break {
            return false;
        }
        if self.group_open {
            return true;
        }
        if last.kind != kind {
            return false;
        }
        match kind {
            EditKind::Typing => typing_touch(last.typing_end, tx),
            EditKind::Deletion => deletion_touch(last.deletion_edge, tx),
            _ => false,
        }
    }

    fn push_new(&mut self, kind: EditKind, part: HistPart) {
        self.coalesce_break = false;
        let mut entry = HistEntry {
            kind,
            parts: Vec::new(),
            typing_end: None,
            deletion_edge: None,
        };
        remember(&mut entry, &part.tx);
        entry.parts.push(part);
        self.undo.push(entry);
    }

    fn append(&mut self, part: HistPart) {
        let Some(entry) = self.undo.last_mut() else {
            self.push_new(EditKind::Programmatic, part);
            return;
        };
        remember(entry, &part.tx);
        entry.parts.push(part);
    }

    fn take_undo(&mut self) -> Option<HistEntry> {
        let entry = self.undo.pop()?;
        self.redo.push(entry.clone());
        Some(entry)
    }

    fn take_redo(&mut self) -> Option<HistEntry> {
        let entry = self.redo.pop()?;
        self.undo.push(entry.clone());
        Some(entry)
    }

    fn begin_group(&mut self) {
        self.group_open = true;
        self.coalesce_break = true;
    }

    fn end_group(&mut self) {
        self.group_open = false;
        self.coalesce_break = true;
    }

    fn mark_saved(&mut self) {
        self.coalesce_break = true;
    }

    fn seal_if_clean(&mut self, clean: bool) {
        if clean {
            self.coalesce_break = true;
        }
    }
}

fn remember(entry: &mut HistEntry, tx: &Transaction) {
    if let Some(edit) = pure_insert(tx) {
        entry.typing_end = edit.start.0.checked_add(scalar_len(&edit.new));
    }
    if let Some(edit) = pure_delete(tx) {
        entry.deletion_edge = Some(edit.start.0);
    }
}

fn typing_touch(end: Option<usize>, tx: &Transaction) -> bool {
    let Some(end) = end else {
        return false;
    };
    pure_insert(tx).is_some_and(|edit| edit.start.0 == end)
}

fn deletion_touch(edge: Option<usize>, tx: &Transaction) -> bool {
    let Some(edge) = edge else {
        return false;
    };
    pure_delete(tx).is_some_and(|edit| edit.old_end().0 == edge)
}

fn insert_kind_of(tx: &Transaction, text: &str) -> EditKind {
    if pure_insert(tx).is_some() && one_cluster(text) {
        EditKind::Typing
    } else {
        EditKind::Paste
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorConfig, Workspace};
    use crate::CloseResult;
    use crate::command::{Command, CommandError};
    use crate::document::{DocumentConfig, TrailingSpaces};
    use crate::movement::{Extend, Movement};
    use crate::selection::{Selection, Selections};
    use crate::view::ViewId;
    use kate_syntax::{Repository, Theme};
    use kate_text::{Edit, Offset, Transaction};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;
    use std::path::Path;
    use std::time::{Duration, Instant};

    const PROPTEST_SEED: u64 = 0x504B_5400_0000_000C;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 128,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn theme() -> Theme {
        Theme::from_json(
            r#"{"metadata":{"name":"T","revision":1,"license":"MIT"},"text-styles":{},"editor-colors":{}}"#,
        )
        .expect("theme json parses")
    }

    fn workspace() -> Workspace {
        Workspace::new(Repository::empty(), theme(), EditorConfig::default())
    }

    fn text_of(ws: &Workspace, doc: crate::DocumentId) -> String {
        ws.documents
            .get(doc)
            .expect("document")
            .buffer
            .text()
            .to_string()
    }

    fn set_caret(ws: &mut Workspace, view: ViewId, at: usize) {
        let state = ws.views.get_mut(view).expect("view");
        state.selections = Selections::single(Selection::caret(Offset(at)));
    }

    #[test]
    fn edit_in_one_view_moves_cursors_in_the_other() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view_a = ws.new_view(doc);
        ws.execute(view_a, Command::InsertText("abcdef".into()))
            .expect("type the base text");
        let view_b = ws.new_view(doc);
        set_caret(&mut ws, view_a, 1);
        set_caret(&mut ws, view_b, 4);
        ws.execute(view_a, Command::InsertText("XYZ".into()))
            .expect("insert before view B");
        let primary = ws.views.get(view_b).expect("view B").selections.primary();
        assert!(primary.is_caret());
        assert_eq!(primary.head, Offset(7));
        assert_eq!(primary.anchor, Offset(7));
        assert_eq!(text_of(&ws, doc), "aXYZbcdef");
    }

    #[test]
    fn open_edit_save_reload_round_trip_in_tempdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"caf\xE9\r\n").expect("write seed file");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open windows-1252 file");
        let view = ws.new_view(doc);
        assert_eq!(text_of(&ws, doc), "caf\u{e9}\n");
        assert_eq!(
            ws.documents
                .get(doc)
                .expect("doc")
                .file
                .as_ref()
                .expect("file")
                .line_ending,
            kate_fileio::LineEnding::CrLf
        );
        ws.execute(view, Command::InsertText("X".into()))
            .expect("insert");
        ws.execute(view, Command::Save).expect("save");
        assert!(!ws.documents.get(doc).expect("doc").history.is_modified());
        let bytes = std::fs::read(&path).expect("read saved bytes");
        assert_eq!(bytes, b"Xcaf\xE9\r\n");
        set_caret(&mut ws, view, 0);
        ws.execute(view, Command::InsertText("Y".into()))
            .expect("dirty the buffer");
        ws.execute(view, Command::Reload).expect("reload");
        let text = text_of(&ws, doc);
        assert_eq!(text, "Xcaf\u{e9}\n");
        assert!(!text.contains('Y'));
        assert!(!ws.documents.get(doc).expect("doc").history.is_modified());
    }

    #[test]
    fn close_last_view_of_modified_document_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"hi\n").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("!".into()))
            .expect("modify");
        assert!(ws.documents.get(doc).expect("doc").history.is_modified());
        assert_eq!(ws.close_view(view), CloseResult::RefusedModified(doc));
        assert!(ws.views.contains_key(view));
        ws.execute(view, Command::Save).expect("save");
        assert!(!ws.documents.get(doc).expect("doc").history.is_modified());
        assert_eq!(ws.close_view(view), CloseResult::Closed);
        assert!(!ws.views.contains_key(view));
        assert!(ws.documents.get(doc).is_none());
    }

    #[test]
    fn undo_of_coalesced_typing_restores_the_other_view() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view_a = ws.new_view(doc);
        ws.execute(view_a, Command::InsertText("0123456789".into()))
            .expect("base");
        let view_b = ws.new_view(doc);
        set_caret(&mut ws, view_a, 2);
        set_caret(&mut ws, view_b, 8);
        for ch in ["a", "b", "c"] {
            ws.execute(view_a, Command::InsertText(ch.into()))
                .expect("type");
        }
        assert_eq!(text_of(&ws, doc), "01abc23456789");
        assert_eq!(
            ws.views.get(view_b).expect("b").selections.primary().head,
            Offset(11)
        );
        ws.execute(view_a, Command::Undo).expect("undo the word");
        assert_eq!(text_of(&ws, doc), "0123456789");
        let restored = ws.views.get(view_b).expect("b").selections.primary();
        assert!(restored.is_caret());
        assert_eq!(restored.head, Offset(8));
        assert_eq!(
            ws.views.get(view_a).expect("a").selections.primary().head,
            Offset(2)
        );
    }

    #[test]
    fn one_undo_reverts_a_group() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::BeginUndoGroup).expect("begin");
        ws.execute(view, Command::InsertText("a".into()))
            .expect("a");
        ws.execute(view, Command::Newline).expect("newline");
        ws.execute(view, Command::InsertText("b".into()))
            .expect("b");
        ws.execute(view, Command::EndUndoGroup).expect("end");
        assert_eq!(text_of(&ws, doc), "a\nb");
        ws.execute(view, Command::Undo).expect("undo group");
        assert_eq!(text_of(&ws, doc), "");
        assert_eq!(
            ws.views.get(view).expect("view").selections.primary().head,
            Offset(0)
        );
    }

    #[test]
    fn insert_does_not_split_a_combining_mark_or_crlf() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("e\u{301}x".into()))
            .expect("seed");
        set_caret(&mut ws, view, 1);
        ws.execute(view, Command::InsertText("Q".into()))
            .expect("insert");
        let text = text_of(&ws, doc);
        assert!(text.contains("e\u{301}"), "{text}");
        assert!(!text.contains("eQ\u{301}"), "{text}");

        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("a\r\nb".into()))
            .expect("seed crlf");
        set_caret(&mut ws, view, 2);
        ws.execute(view, Command::InsertText("Q".into()))
            .expect("insert inside crlf");
        let text = text_of(&ws, doc);
        assert!(text.contains("\r\n"), "{text}");
        assert!(!text.contains("\rQ\n"), "{text}");
    }

    #[test]
    fn backspace_deletes_a_whole_crlf_pair() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("a\r\nb".into()))
            .expect("seed");
        set_caret(&mut ws, view, 3);
        ws.execute(view, Command::DeleteBackward)
            .expect("backspace");
        assert_eq!(text_of(&ws, doc), "ab");
    }

    #[test]
    fn commands_do_not_panic_on_bad_input() {
        let mut ws = workspace();
        let missing = ViewId::default();
        assert!(matches!(
            ws.execute(missing, Command::Undo),
            Err(CommandError::UnknownView(_))
        ));
        assert_eq!(ws.close_view(missing), CloseResult::Closed);
        let err = ws
            .open(Path::new("/no/such/pkt-0012-file"))
            .expect_err("missing");
        assert!(matches!(err, super::OpenError::Load(_)));
        let bogus = ws.new_view(crate::DocumentId::default());
        assert!(
            ws.execute(bogus, Command::Move(Movement::Left, Extend::No))
                .is_err()
        );
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        assert!(matches!(
            ws.execute(view, Command::Save),
            Err(CommandError::NoFile)
        ));
        assert!(matches!(
            ws.execute(view, Command::Reload),
            Err(CommandError::NoFile)
        ));
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, [0u8, 1, 2, 3]).expect("write binary");
        assert!(matches!(
            ws.open(&path),
            Err(super::OpenError::ProbablyBinary(_))
        ));
        assert_eq!(ws.documents.len(), 1);
    }

    #[test]
    fn open_reuses_the_canonical_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("same.txt");
        std::fs::write(&path, b"one\n").expect("write");
        let mut ws = workspace();
        let first = ws.open(&path).expect("open");
        let second = ws.open(&path).expect("open again");
        assert_eq!(first, second);
        assert_eq!(ws.documents.len(), 1);
    }

    #[test]
    fn scroll_clamps_to_the_buffer() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("a\nb\nc\n".into()))
            .expect("lines");
        ws.execute(view, Command::Scroll(100)).expect("scroll");
        let last = ws.documents.get(doc).expect("doc").buffer.line_count() - 1;
        assert_eq!(ws.views.get(view).expect("view").scroll.first_line, last);
        ws.execute(view, Command::Scroll(-1_000))
            .expect("scroll back");
        assert_eq!(ws.views.get(view).expect("view").scroll.first_line, 0);
        ws.execute(view, Command::ScrollTo(1)).expect("scroll to");
        assert_eq!(ws.views.get(view).expect("view").scroll.first_line, 1);
    }

    #[test]
    fn add_cursor_below_uses_the_next_line() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        ws.execute(view, Command::InsertText("ab\ncd".into()))
            .expect("lines");
        set_caret(&mut ws, view, 0);
        ws.execute(view, Command::AddCursorBelow).expect("add");
        let sel = &ws.views.get(view).expect("view").selections;
        assert_eq!(sel.len(), 2);
        let heads: Vec<Offset> = sel.iter().map(|item| item.head).collect();
        assert_eq!(heads, vec![Offset(0), Offset(3)]);
    }

    #[test]
    fn modified_lines_lose_trailing_spaces_on_save() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pad.txt");
        std::fs::write(&path, b"a \nb \n").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        ws.documents
            .get_mut(doc)
            .expect("doc")
            .config
            .remove_trailing_spaces = TrailingSpaces::ModifiedLines;
        set_caret(&mut ws, view, 0);
        ws.execute(view, Command::InsertText("Z".into()))
            .expect("edit line 0");
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(text_of(&ws, doc), "Za\nb \n");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"Za\nb \n");
    }

    #[test]
    fn modified_lines_strips_a_shifted_line_after_undo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("shift.txt");
        std::fs::write(&path, b"top\ntail  ").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        ws.documents
            .get_mut(doc)
            .expect("doc")
            .config
            .remove_trailing_spaces = TrailingSpaces::ModifiedLines;
        set_caret(&mut ws, view, 4);
        ws.execute(view, Command::InsertText("Z".into()))
            .expect("type on the last line");
        set_caret(&mut ws, view, 0);
        ws.execute(view, Command::InsertText("\n".into()))
            .expect("newline at the top");
        ws.execute(view, Command::Undo).expect("undo the newline");
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(text_of(&ws, doc), "top\nZtail");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"top\nZtail");
    }

    #[test]
    fn save_can_append_a_final_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nl.txt");
        std::fs::write(&path, b"abc").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        ws.documents
            .get_mut(doc)
            .expect("doc")
            .config
            .ensure_trailing_newline = true;
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(text_of(&ws, doc), "abc\n");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"abc\n");
        let _ = DocumentConfig::default();
    }

    #[test]
    fn typing_with_twenty_thousand_carets_stays_fast() {
        const COUNT: usize = 20_000;
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        let text = "a".repeat(COUNT * 2);
        ws.execute(view, Command::InsertText(text)).expect("seed");
        let ranges: Vec<Selection> = (0..COUNT)
            .map(|index| Selection::caret(Offset(index * 2)))
            .collect();
        ws.views.get_mut(view).expect("view").selections = Selections::new(ranges, 0);
        let started = Instant::now();
        ws.execute(view, Command::InsertText("x".into()))
            .expect("type at many carets");
        let elapsed = started.elapsed();
        let sel = &ws.views.get(view).expect("view").selections;
        assert_eq!(sel.len(), COUNT);
        assert!(sel.iter().all(|item| item.is_caret()));
        assert_eq!(sel.primary().head, Offset(1));
        assert!(
            elapsed < Duration::from_secs(10),
            "typing at {COUNT} carets took {elapsed:?}"
        );
    }

    #[test]
    fn many_views_see_one_insert() {
        const VIEWS: usize = 1_000;
        let mut ws = workspace();
        let doc = ws.new_document();
        let first = ws.new_view(doc);
        ws.execute(first, Command::InsertText("a".repeat(4_000)))
            .expect("seed");
        let mut ids = Vec::with_capacity(VIEWS);
        ids.push(first);
        for index in 1..VIEWS {
            let view = ws.new_view(doc);
            set_caret(&mut ws, view, 10 + index);
            ids.push(view);
        }
        set_caret(&mut ws, first, 0);
        let started = Instant::now();
        ws.execute(first, Command::InsertText("xyz".into()))
            .expect("insert");
        let elapsed = started.elapsed();
        for (index, view) in ids.iter().enumerate().skip(1) {
            let primary = ws.views.get(*view).expect("view").selections.primary();
            assert!(primary.is_caret());
            assert_eq!(primary.head, Offset(10 + index + 3));
        }
        assert!(
            elapsed < Duration::from_secs(10),
            "{VIEWS} views took {elapsed:?}"
        );
    }

    #[test]
    fn keystroke_on_a_large_document_stays_fast() {
        const LINES: usize = 100_000;
        let mut ws = workspace();
        ws.syntax
            .add_xml(include_str!("../../kate-syntax/definitions/rust.xml"))
            .expect("rust");
        for xml in [
            include_str!("../../kate-syntax/definitions/comments.xml"),
            include_str!("../../kate-syntax/definitions/alert.xml"),
            include_str!("../../kate-syntax/definitions/modelines.xml"),
            include_str!("../../kate-syntax/definitions/spdx-comments.xml"),
        ] {
            ws.syntax.add_xml(xml).expect("syntax xml");
        }
        ws.syntax.link().expect("link");
        let id = ws.syntax.by_name("Rust").expect("Rust");
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        let mut seed = String::with_capacity(LINES * 8);
        for index in 0..LINES {
            if index > 0 {
                seed.push('\n');
            }
            seed.push_str("let v = 1;");
        }
        ws.execute(view, Command::InsertText(seed)).expect("seed");
        ws.execute(view, Command::SetSyntax(Some(id)))
            .expect("syntax");
        set_caret(&mut ws, view, 0);
        let started = Instant::now();
        ws.execute(view, Command::InsertText("x".into()))
            .expect("key");
        let elapsed = started.elapsed();
        assert!(text_of(&ws, doc).starts_with('x'));
        assert_eq!(
            ws.documents.get(doc).expect("doc").buffer.line_count(),
            LINES as u32
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "keystroke on {LINES} lines took {elapsed:?}"
        );
    }

    #[test]
    fn other_view_caret_moves_past_an_insert() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view_a = ws.new_view(doc);
        ws.execute(view_a, Command::InsertText("hello world".into()))
            .expect("seed");
        let view_b = ws.new_view(doc);
        set_caret(&mut ws, view_a, 5);
        set_caret(&mut ws, view_b, 5);
        ws.execute(view_a, Command::InsertText(",".into()))
            .expect("comma");
        let other = ws.views.get(view_b).expect("b").selections.primary();
        assert!(other.is_caret());
        assert_eq!(other.head, Offset(6));
        ws.execute(view_b, Command::InsertText("!".into()))
            .expect("bang");
        assert_eq!(text_of(&ws, doc), "hello,! world");
        let first = ws.views.get(view_a).expect("a").selections.primary();
        assert!(first.is_caret());
        assert_eq!(first.head, Offset(7));
    }

    #[test]
    fn mixed_set_caret_at_an_insert_stays_a_caret() {
        let sel = Selections::new(
            vec![
                Selection {
                    anchor: Offset(0),
                    head: Offset(2),
                    goal_col: None,
                },
                Selection::caret(Offset(2)),
            ],
            1,
        );
        let tx = Transaction::single(Edit::insert(Offset(2), "x"));
        let after = super::transform_foreign(&sel, &tx);
        assert_eq!(after.len(), 2);
        assert!(after.primary().is_caret());
        assert_eq!(after.primary().anchor, Offset(3));
        assert_eq!(after.primary().head, Offset(3));
        let range = after.iter().find(|item| !item.is_caret()).expect("range");
        assert_eq!(range.anchor, Offset(0));
        assert_eq!(range.head, Offset(3));
    }

    #[test]
    fn other_view_mixed_set_keeps_a_caret_past_an_insert() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view_a = ws.new_view(doc);
        ws.execute(view_a, Command::InsertText("hello world".into()))
            .expect("seed");
        let view_b = ws.new_view(doc);
        set_caret(&mut ws, view_a, 5);
        ws.views.get_mut(view_b).expect("b").selections = Selections::new(
            vec![
                Selection {
                    anchor: Offset(0),
                    head: Offset(3),
                    goal_col: None,
                },
                Selection::caret(Offset(5)),
            ],
            1,
        );
        ws.execute(view_a, Command::InsertText(",".into()))
            .expect("comma");
        let other = &ws.views.get(view_b).expect("b").selections;
        assert_eq!(other.len(), 2);
        assert!(other.primary().is_caret());
        assert_eq!(other.primary().anchor, Offset(6));
        assert_eq!(other.primary().head, Offset(6));
        let range = other.iter().find(|item| !item.is_caret()).expect("range");
        assert_eq!(range.anchor, Offset(0));
        assert_eq!(range.head, Offset(3));
        ws.execute(view_b, Command::InsertText("!".into()))
            .expect("bang");
        assert_eq!(text_of(&ws, doc), "!lo,! world");
        let first = ws.views.get(view_a).expect("a").selections.primary();
        assert!(first.is_caret());
        assert_eq!(first.head, Offset(5));
    }

    #[test]
    fn selection_end_inside_a_grapheme_snaps_up() {
        let buf = kate_text::TextBuffer::from_str("e\u{301}x");
        let mut sel = Selections::single(Selection {
            anchor: Offset(0),
            head: Offset(1),
            goal_col: None,
        });
        super::snap_set(&buf, &mut sel);
        let primary = sel.primary();
        assert_eq!(primary.anchor, Offset(0));
        assert_eq!(primary.head, Offset(2));
    }

    #[test]
    fn strip_removes_trailing_tabs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tab.txt");
        std::fs::write(&path, b"ab\t").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        {
            let config = &mut ws.documents.get_mut(doc).expect("doc").config;
            config.remove_trailing_spaces = TrailingSpaces::All;
            config.ensure_trailing_newline = false;
        }
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"ab");
    }

    #[test]
    fn save_strips_then_adds_one_final_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pad.txt");
        std::fs::write(&path, b"abc\t \n  ").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        {
            let config = &mut ws.documents.get_mut(doc).expect("doc").config;
            config.remove_trailing_spaces = TrailingSpaces::All;
            config.ensure_trailing_newline = true;
        }
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"abc\n");
        assert_eq!(text_of(&ws, doc), "abc\n");
    }

    #[test]
    fn save_of_an_empty_document_does_not_add_a_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, b"").expect("write");
        let mut ws = workspace();
        let doc = ws.open(&path).expect("open");
        let view = ws.new_view(doc);
        ws.documents
            .get_mut(doc)
            .expect("doc")
            .config
            .ensure_trailing_newline = true;
        ws.execute(view, Command::Save).expect("save");
        assert_eq!(std::fs::read(&path).expect("bytes"), b"");
    }

    #[test]
    fn modified_lines_with_twenty_thousand_carets_stays_fast() {
        const COUNT: usize = 20_000;
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc);
        let mut seed = String::with_capacity(COUNT * 2);
        for index in 0..COUNT {
            if index > 0 {
                seed.push('\n');
            }
            seed.push('a');
        }
        ws.execute(view, Command::InsertText(seed)).expect("seed");
        ws.documents
            .get_mut(doc)
            .expect("doc")
            .config
            .remove_trailing_spaces = TrailingSpaces::ModifiedLines;
        let ranges: Vec<Selection> = (0..COUNT)
            .map(|index| Selection::caret(Offset(index * 2)))
            .collect();
        ws.views.get_mut(view).expect("view").selections = Selections::new(ranges, 0);
        ws.execute(view, Command::InsertText("x".into()))
            .expect("first key");
        let ranges: Vec<Selection> = (0..COUNT)
            .map(|index| Selection::caret(Offset(index * 3)))
            .collect();
        ws.views.get_mut(view).expect("view").selections = Selections::new(ranges, 0);
        let started = Instant::now();
        ws.execute(view, Command::InsertText("y".into()))
            .expect("second key");
        let elapsed = started.elapsed();
        assert_eq!(ws.extra.get(&doc).expect("extra").touched.len(), COUNT);
        assert!(text_of(&ws, doc).starts_with("yx"));
        assert!(
            elapsed < Duration::from_secs(3),
            "ModifiedLines at {COUNT} carets took {elapsed:?}"
        );
    }

    #[test]
    fn mixed_selections_in_the_other_view_stay_fast() {
        const COUNT: usize = 20_000;
        let mut ws = workspace();
        let doc = ws.new_document();
        let view_a = ws.new_view(doc);
        let mut seed = String::with_capacity(COUNT * 3);
        for index in 0..COUNT {
            if index > 0 {
                seed.push('\n');
            }
            seed.push_str("ab");
        }
        ws.execute(view_a, Command::InsertText(seed.clone()))
            .expect("seed");
        let view_b = ws.new_view(doc);
        let carets: Vec<Selection> = (0..COUNT)
            .map(|index| Selection::caret(Offset(index * 3)))
            .collect();
        ws.views.get_mut(view_a).expect("a").selections = Selections::new(carets, 0);
        let mixed: Vec<Selection> = (0..COUNT)
            .map(|index| {
                let start = index * 3;
                if index % 2 == 0 {
                    Selection::caret(Offset(start))
                } else {
                    Selection {
                        anchor: Offset(start),
                        head: Offset(start + 2),
                        goal_col: None,
                    }
                }
            })
            .collect();
        ws.views.get_mut(view_b).expect("b").selections = Selections::new(mixed, 1);
        assert_eq!(ws.views.get(view_b).expect("b").selections.len(), COUNT);
        let started = Instant::now();
        ws.execute(view_a, Command::InsertText("x".into()))
            .expect("type");
        let typed = started.elapsed();
        let other = &ws.views.get(view_b).expect("b").selections;
        assert_eq!(other.len(), COUNT);
        assert!(other.iter().enumerate().all(|(index, selection)| {
            if index % 2 == 0 {
                selection.is_caret()
            } else {
                !selection.is_caret()
            }
        }));
        let started = Instant::now();
        ws.execute(view_a, Command::Undo).expect("undo");
        let undone = started.elapsed();
        assert_eq!(text_of(&ws, doc), seed);
        let other = &ws.views.get(view_b).expect("b").selections;
        assert_eq!(other.len(), COUNT);
        assert!(other.iter().enumerate().all(|(index, selection)| {
            if index % 2 == 0 {
                selection.is_caret()
            } else {
                !selection.is_caret()
            }
        }));
        assert!(
            typed < Duration::from_secs(3),
            "mixed foreign type at {COUNT} took {typed:?}"
        );
        assert!(
            undone < Duration::from_secs(3),
            "mixed foreign undo at {COUNT} took {undone:?}"
        );
    }

    fn arb_text() -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(vec!['a', 'b', 'é', '\n', ' ']), 0..24)
            .prop_map(|chars| chars.into_iter().collect())
    }

    fn arb_insert() -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(vec!['x', 'y', 'é', '\n']), 1..5)
            .prop_map(|chars| chars.into_iter().collect())
    }

    fn string_oracle(text: &str, at: usize, other: usize, insert: &str) -> (String, usize, usize) {
        let mut chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let at = at.min(len);
        let other = other.min(len);
        let extra: Vec<char> = insert.chars().collect();
        for (index, ch) in extra.iter().enumerate() {
            chars.insert(at + index, *ch);
        }
        let anchor_head = if other < at {
            (other, other)
        } else if other > at {
            (other + extra.len(), other + extra.len())
        } else {
            let past = other + extra.len();
            (past, past)
        };
        (chars.into_iter().collect(), anchor_head.0, anchor_head.1)
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn other_view_tracks_an_insert_like_the_string_oracle(
            (text, at, other, insert) in prop_oneof![
                1 => Just(("abcd".to_string(), 1usize, 3usize, "XYZ".to_string())),
                8 => (arb_text(), 0usize..30, 0usize..30, arb_insert()),
            ]
        ) {
            let mut ws = workspace();
            let doc = ws.new_document();
            let view_a = ws.new_view(doc);
            if !text.is_empty() {
                ws.execute(view_a, Command::InsertText(text.clone())).expect("seed");
            }
            let view_b = ws.new_view(doc);
            let len = text.chars().count();
            set_caret(&mut ws, view_a, at.min(len));
            set_caret(&mut ws, view_b, other.min(len));
            ws.execute(view_a, Command::InsertText(insert.clone())).expect("insert");
            let (expected, anchor, head) = string_oracle(&text, at, other, &insert);
            prop_assert_eq!(text_of(&ws, doc), expected);
            let primary = ws.views.get(view_b).expect("b").selections.primary();
            prop_assert_eq!(primary.anchor, Offset(anchor));
            prop_assert_eq!(primary.head, Offset(head));
        }
    }
}
