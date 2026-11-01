// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Commands that change a workspace.
//!
//! Key bindings, menus, and tests all send a [`Command`].
//! [`crate::Workspace::execute`] is the only mutation path.

use std::path::PathBuf;

use kate_fileio::{LoadError, SaveError};
use kate_syntax::DefId;
use kate_text::{ApplyError, TransactionError};

use crate::document::MarkKind;
use crate::movement::{Extend, Movement};
use crate::view::ViewId;

/// One request from a key binding, a menu, or a test.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    /// Replace each selection with this text.
    InsertText(String),
    /// Insert a line break at each selection.
    Newline,
    /// Delete one grapheme before a caret, or delete a selection.
    DeleteBackward,
    /// Delete one grapheme after a caret, or delete a selection.
    DeleteForward,
    /// Move every selection.
    Move(Movement, Extend),
    /// Select the whole buffer.
    SelectAll,
    /// Add a caret on the next line for each selection.
    AddCursorBelow,
    /// Add a caret on the previous line for each selection.
    AddCursorAbove,
    /// Keep the primary selection.
    RemoveSecondaryCursors,
    /// Undo one history entry.
    Undo,
    /// Redo one history entry.
    Redo,
    /// Start an undo group.
    BeginUndoGroup,
    /// End an undo group.
    EndUndoGroup,
    /// Write the document to its path.
    Save,
    /// Write the document to a new path.
    SaveAs(PathBuf),
    /// Read the path again and replace the buffer.
    Reload,
    /// Change the syntax definition. `None` clears it.
    SetSyntax(Option<DefId>),
    /// Add or remove a mark on a line.
    ToggleMark(MarkKind, u32),
    /// Move the scroll line by a delta.
    Scroll(i32),
    /// Set the scroll line.
    ScrollTo(u32),
}

/// `execute` rejected a command.
#[derive(Debug)]
pub enum CommandError {
    /// Save of a document that has no path.
    NoFile,
    /// The save failed.
    Save(SaveError),
    /// The load failed.
    Load(LoadError),
    /// The buffer rejected the transaction.
    Apply(ApplyError),
    /// The edit list overlaps.
    Transaction(TransactionError),
    /// The view id is not in the workspace.
    UnknownView(ViewId),
}
