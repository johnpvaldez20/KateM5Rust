// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-core`: the editor model without a GUI.
//!
//! Documents, views, selections (one cursor is a selection set of size one),
//! marks, folding, indentation, the command table and configuration. The GUI
//! crate renders this state and feeds input back as commands; it never
//! reaches around it.
//!
//! Specified in `docs/design/01-data-structures.md`, section "Document and
//! view model". Arrives with packets PKT-0011 and PKT-0012.
