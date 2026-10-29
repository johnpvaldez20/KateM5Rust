//! `kate-text`: the text storage layer.
//!
//! Owns the rope buffer, flat character offsets, invertible edits grouped into
//! transactions, the revision log, and anchors that survive edits.
//! Nothing here knows about files, syntax, views or the GUI.
//!
//! The types are specified in `docs/design/01-data-structures.md` and arrive
//! with packets PKT-0002 to PKT-0004. This file is intentionally empty until
//! then: no speculative code.
