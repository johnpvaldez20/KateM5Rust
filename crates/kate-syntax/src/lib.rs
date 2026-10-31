// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! `kate-syntax`: syntax highlighting.
//!
//! A Rust implementation of the KSyntaxHighlighting definition format
//! (`language.xsd`): definition loading, the context-stack highlighting
//! engine, and theme loading. The engine works on one line at a time plus an
//! opaque state; it does not depend on `kate-text`.
//!
//! Specified in `docs/design/01-data-structures.md`, section "Syntax
//! highlighting". Arrives with packets PKT-0007 to PKT-0009.

mod definition;
mod engine;
mod load;
mod regex;
mod repository;
mod state;

pub use definition::{
    CommentPosition, CommentStyle, Context, ContextId, ContextSwitch, DefId, DefaultStyle,
    Definition, FoldId, Format, FormatId, KeywordList, KeywordListId, Rgb, Rule, RuleKind,
    WordDelimiters,
};
pub use engine::{FoldEvent, Highlighter, LineHighlight, Span};
pub use regex::{RegexCache, RegexError, translate_pattern};
pub use repository::{LoadError, Repository};
pub use state::{Frame, State, StateId, StateInterner};
