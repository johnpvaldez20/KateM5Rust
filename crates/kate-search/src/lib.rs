// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! `kate-search`: plain-text and regular-expression search over a rope.
//!
//! Produces match ranges in buffer offsets and builds replace transactions.
//! Depends on `kate-text` only.
//!
//! Specified in `docs/design/01-data-structures.md`, section "Search".
//! Arrives with packet PKT-0010.
