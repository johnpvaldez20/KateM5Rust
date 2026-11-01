// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-core`: the editor model without a GUI.
//!
//! A workspace owns documents and views.
//! A document owns the text, the undo history, the file state, and the syntax cache.
//! A view owns selections and scroll state.
//! `Workspace::execute` is the mutation path.
//! The GUI renders this state. It does not change a buffer except through `execute`.

mod command;
mod document;
mod fanout;
mod movement;
mod selection;
mod syntax_cache;
mod view;
mod workspace;

pub use command::{Command, CommandError};
pub use document::{
    DiskChange, Document, DocumentConfig, DocumentId, FileState, Mark, MarkKind, TrailingSpaces,
};
pub use fanout::{delete_backward, delete_forward, fan_out, insert_text};
pub use movement::{Extend, MoveContext, Movement, move_all, move_selection};
pub use selection::{Selection, Selections};
pub use syntax_cache::SyntaxCache;
pub use view::{Scroll, View, ViewId};
pub use workspace::{Axis, CloseResult, EditorConfig, OpenError, SplitTree, Workspace};
