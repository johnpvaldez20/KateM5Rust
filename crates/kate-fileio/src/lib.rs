// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! File loading, encoding, and saving for the editor.
//!
//! `decode` turns bytes into a rope. It checks a BOM, then strict UTF-8,
//! then the fallback encoding. It rewrites CR LF and a lone CR to LF.
//! `load` reads one path and adds the size, the mtime, and a SHA-256 digest.
//! `encode` converts a rope to bytes with a chosen encoding, BOM, and line ending.
//! `save` writes to a temp file, fsyncs, copies mode bits, and renames.

mod decode;
mod encode;
mod load;
mod save;

pub use decode::{DecodeError, DecodeOptions, Decoded, Encoding, LineEnding, decode};
pub use encode::{EncodeError, EncodeOptions, encode};
pub use load::{LoadError, Loaded, load};
pub use save::{SaveError, SaveStep, Saved, save};
