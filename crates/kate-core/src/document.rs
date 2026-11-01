// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! One open document.
//!
//! The document owns the text, the undo history, the file state, and the syntax cache.
//! A view does not own those values. It points at a [`DocumentId`].

use std::path::PathBuf;
use std::time::SystemTime;

use kate_fileio::{Encoding, LineEnding};
use kate_text::{Anchor, History, TextBuffer};

use crate::syntax_cache::SyntaxCache;

slotmap::new_key_type! {
    /// Key for one document in a [`crate::Workspace`].
    pub struct DocumentId;
}

/// One open buffer and the state that belongs to it.
pub struct Document {
    /// The rope and the edit log.
    pub buffer: TextBuffer,
    /// Undo and redo for `buffer`.
    pub history: History,
    /// `None` when the document has no path.
    pub file: Option<FileState>,
    /// Per-line highlight state.
    pub syntax: SyntaxCache,
    /// Marks anchored at line starts.
    pub marks: Vec<Mark>,
    /// Name shown for the document.
    pub name: String,
    /// Indent and save policy for this document.
    pub config: DocumentConfig,
}

/// Bytes on disk for a document that has a path.
#[derive(Clone, Debug)]
pub struct FileState {
    /// Canonical path when the platform can resolve one.
    pub path: PathBuf,
    /// Encoding used to read and write the file.
    pub encoding: Encoding,
    /// `true` when the file starts with a byte-order mark.
    pub bom: bool,
    /// Line ending written on save.
    pub line_ending: LineEnding,
    /// SHA-256 of the bytes last read or written.
    pub digest_on_disk: [u8; 32],
    /// Modified time last read or written.
    pub mtime_on_disk: Option<SystemTime>,
    /// Set when a later packet sees a disk change.
    pub disk_change: Option<DiskChange>,
}

/// How the file on disk differs from the open buffer.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiskChange {
    /// The bytes on disk changed.
    Modified,
    /// The path is gone.
    Deleted,
    /// The path exists again.
    Created,
}

/// A mark anchored at a line start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mark {
    /// Position that moves with the text.
    pub anchor: Anchor,
    /// Kind of mark.
    pub kind: MarkKind,
}

/// The kind of a mark.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MarkKind {
    /// A bookmark.
    Bookmark,
    /// An active breakpoint.
    BreakpointActive,
    /// A disabled breakpoint.
    BreakpointDisabled,
    /// The current execution line.
    Execution,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// Indent and trailing-space policy for one document.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentConfig {
    /// Width of one tab stop. `0` is treated as `1` by movement.
    pub tab_width: u8,
    /// Columns added by an indent step.
    pub indent_width: u8,
    /// `true` when an indent step uses spaces.
    pub indent_with_spaces: bool,
    /// Which lines lose trailing spaces on save.
    pub remove_trailing_spaces: TrailingSpaces,
    /// `true` when save adds a final line ending if the buffer has none.
    pub ensure_trailing_newline: bool,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            indent_width: 4,
            indent_with_spaces: true,
            remove_trailing_spaces: TrailingSpaces::Never,
            ensure_trailing_newline: false,
        }
    }
}

/// Which lines lose trailing spaces on save.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TrailingSpaces {
    /// Do not remove trailing spaces.
    Never,
    /// Remove trailing spaces on lines edited since the last save.
    ModifiedLines,
    /// Remove trailing spaces on every line.
    All,
}

impl Document {
    pub(crate) fn blank(name: String, config: DocumentConfig) -> Self {
        let buffer = TextBuffer::new();
        let mut syntax = SyntaxCache::new(None);
        syntax.fit(buffer.line_count());
        Self {
            buffer,
            history: History::new(),
            file: None,
            syntax,
            marks: Vec::new(),
            name,
            config,
        }
    }

    pub(crate) fn from_loaded(
        name: String,
        config: DocumentConfig,
        text: &str,
        file: FileState,
        definition: Option<kate_syntax::DefId>,
    ) -> Self {
        let buffer = TextBuffer::from_str(text);
        let mut syntax = SyntaxCache::new(definition);
        syntax.fit(buffer.line_count());
        Self {
            buffer,
            history: History::new(),
            file: Some(file),
            syntax,
            marks: Vec::new(),
            name,
            config,
        }
    }
}
