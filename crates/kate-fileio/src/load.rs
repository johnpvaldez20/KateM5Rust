// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Read one path into a decoded rope plus a digest of the bytes on disk.

use std::path::Path;
use std::time::SystemTime;

use crate::decode::{DecodeError, DecodeOptions, Decoded, decode};

/// A file plus the bytes-on-disk facts needed for a later modified check.
#[derive(Debug)]
pub struct Loaded {
    pub decoded: Decoded,
    pub size_bytes: u64,
    /// `None` when the platform does not report a modified time.
    pub mtime: Option<SystemTime>,
    /// SHA-256 of the bytes on disk.
    pub digest: [u8; 32],
}

/// `load` failed while reading the path or while decoding the bytes.
#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Decode(DecodeError),
}

/// Reads `path` and decodes the bytes with `opts`.
pub fn load(path: &Path, opts: &DecodeOptions) -> Result<Loaded, LoadError> {
    let meta = std::fs::metadata(path).map_err(LoadError::Io)?;
    if meta.len() > opts.max_bytes {
        return Err(LoadError::Decode(DecodeError::TooLarge {
            size: meta.len(),
            limit: opts.max_bytes,
        }));
    }
    let mtime = meta.modified().ok();
    let bytes = std::fs::read(path).map_err(LoadError::Io)?;
    let decoded = decode(&bytes, opts).map_err(LoadError::Decode)?;
    Ok(Loaded {
        decoded,
        size_bytes: u64::try_from(bytes.len()).expect("byte length fits in u64"),
        mtime,
        digest: sha256(&bytes),
    })
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

#[cfg(test)]
mod tests {
    use super::load;
    use crate::decode::DecodeOptions;
    use sha2::{Digest, Sha256};

    #[test]
    fn load_reads_tempdir_file_and_digests_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let bytes = b"hello\n";
        std::fs::write(&path, bytes).unwrap();

        let loaded = load(&path, &DecodeOptions::default()).unwrap();
        assert_eq!(loaded.decoded.text.to_string(), "hello\n");
        assert_eq!(loaded.size_bytes, u64::try_from(bytes.len()).unwrap());
        assert!(loaded.mtime.is_some());
        assert_eq!(loaded.digest, sha256(bytes));
        assert_eq!(loaded.decoded.line_ending, crate::decode::LineEnding::Lf);
        assert!(!loaded.decoded.bom);
        assert_eq!(loaded.decoded.encoding, encoding_rs::UTF_8);
    }

    fn sha256(bytes: &[u8]) -> [u8; 32] {
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        out
    }
}
