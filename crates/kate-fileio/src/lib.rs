// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! File loading for the editor.
//!
//! `decode` turns bytes into a rope. It checks a BOM, then strict UTF-8,
//! then the fallback encoding. It rewrites CR LF and a lone CR to LF.
//! `load` reads one path and adds the size, the mtime, and a SHA-256 digest.
//! Encode and save arrive in a later packet.

mod decode;
mod load;

pub use decode::{DecodeError, DecodeOptions, Decoded, Encoding, LineEnding, decode};
pub use load::{LoadError, Loaded, load};
