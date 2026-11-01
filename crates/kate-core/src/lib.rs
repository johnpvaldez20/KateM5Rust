// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-core`: the editor model without a GUI.
//!
//! A selection set, grapheme movement, and edit fan-out live here.
//! Documents, views, marks, and commands arrive with later packets.
//! The GUI renders this state. It does not reach around it.

mod fanout;
mod movement;
mod selection;

pub use fanout::{delete_backward, delete_forward, fan_out, insert_text};
pub use movement::{Extend, MoveContext, Movement, move_all, move_selection};
pub use selection::{Selection, Selections};
