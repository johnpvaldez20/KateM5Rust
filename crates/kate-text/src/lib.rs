// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Text storage for the editor.
//!
//! `TextBuffer` holds a rope. Offsets count Unicode scalar values.
//! A transaction is a sorted list of edits against one pre-state.
//! `apply` checks every edit, applies the whole list, and increases the revision by 1.
//! An insert has an empty old side. A delete has an empty new side.
//! An anchor stores an offset, a revision, and a bias.
//! Resolve replays the edit log.
//! `History` records undo and redo.
//! A file load is a later packet.

mod anchor;
mod buffer;
mod edit;
mod history;
mod log;
mod offset;

pub use anchor::{Anchor, Bias, StaleAnchor, transform_offset};
pub use buffer::{ApplyError, TextBuffer};
pub use edit::{Edit, Transaction, TransactionError};
pub use history::{EditKind, History, HistoryEntry};
pub use log::EditLog;
pub use offset::{CharRange, LineCol, Offset, Revision};
