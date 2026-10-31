// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Atomic file save: encode, write to a temp file, fsync, copy mode, rename.
//!
//! No fallback to non-atomic writes. If the directory is not writable,
//! the error names the step that failed.

use crate::{EncodeOptions, encode};
use ropey::Rope;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Result of a successful save.
#[derive(Debug)]
pub struct Saved {
    pub bytes_written: u64,
    pub digest: [u8; 32],
    pub mtime: Option<std::time::SystemTime>,
}

/// Error returned by [`save`].
#[derive(Debug)]
pub enum SaveError {
    Encode(crate::EncodeError),
    Io {
        step: SaveStep,
        source: std::io::Error,
    },
}

/// Identifies which I/O step failed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SaveStep {
    CreateTemp,
    Write,
    Sync,
    CopyMode,
    Rename,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes to `<dir>/.<name>.katem5-<pid>-<counter>`, fsyncs, copies the
/// target's mode bits if the target exists, then renames over the target.
///
/// Returns `Saved` with the byte count, SHA-256 digest, and mtime of the
/// written file.
pub fn save(path: &Path, text: &Rope, opts: &EncodeOptions) -> Result<Saved, SaveError> {
    let bytes = encode(text, opts).map_err(SaveError::Encode)?;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".{file_name}.katem5-{}-{counter}", std::process::id());
    let temp_path = dir.join(temp_name);

    let result = do_save(&temp_path, path, &bytes);
    if let Err(SaveError::Io {
        step: SaveStep::CreateTemp,
        ..
    }) = &result
    {
        // Temp file was never created; nothing to clean up.
    } else if result.is_err() {
        // Best-effort cleanup; the file exists but a later step failed.
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn do_save(temp_path: &Path, target: &Path, bytes: &[u8]) -> Result<Saved, SaveError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|e| SaveError::Io {
            step: SaveStep::CreateTemp,
            source: e,
        })?;

    file.write_all(bytes).map_err(|e| SaveError::Io {
        step: SaveStep::Write,
        source: e,
    })?;

    file.sync_all().map_err(|e| SaveError::Io {
        step: SaveStep::Sync,
        source: e,
    })?;

    // Copy mode bits from the existing target, if any.
    if let Ok(meta) = std::fs::metadata(target) {
        file.set_permissions(meta.permissions())
            .map_err(|e| SaveError::Io {
                step: SaveStep::CopyMode,
                source: e,
            })?;
    }

    drop(file);

    std::fs::rename(temp_path, target).map_err(|e| SaveError::Io {
        step: SaveStep::Rename,
        source: e,
    })?;

    let mtime = std::fs::metadata(target)
        .ok()
        .and_then(|m| m.modified().ok());

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&hash);

    Ok(Saved {
        bytes_written: u64::try_from(bytes.len()).expect("byte length fits in u64"),
        digest,
        mtime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LineEnding;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn utf8_opts(ending: LineEnding) -> EncodeOptions {
        EncodeOptions {
            encoding: encoding_rs::UTF_8,
            bom: false,
            line_ending: ending,
            ensure_trailing_newline: false,
        }
    }

    #[test]
    fn rename_over_existing_preserves_mode_bits() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("test.txt");

        // Create target with mode 0o640.
        std::fs::write(&target, b"original content").expect("write target");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640))
            .expect("set mode");

        // Save new content over it.
        let rope = Rope::from_str("new content\n");
        let saved = save(&target, &rope, &utf8_opts(LineEnding::Lf)).expect("save");

        // Mode bits preserved.
        let meta = std::fs::metadata(&target).expect("metadata");
        assert_eq!(meta.permissions().mode() & 0o777, 0o640);

        // Content matches what we encoded.
        let on_disk = std::fs::read(&target).expect("read target");
        assert_eq!(&on_disk, b"new content\n");
        assert_eq!(saved.bytes_written, b"new content\n".len() as u64);

        // No leftover temp files in the directory.
        for entry in std::fs::read_dir(dir.path()).expect("read dir") {
            let name = entry.expect("dir entry").file_name();
            assert!(!name.to_string_lossy().starts_with(".test.txt.katem5-"));
        }
    }

    #[test]
    fn failed_write_leaves_target_untouched() {
        let dir = tempdir().expect("tempdir");
        // Use a subdirectory so the tempdir root stays writable for cleanup.
        let subdir = dir.path().join("readonly");
        std::fs::create_dir(&subdir).expect("create subdir");

        let target = subdir.join("locked.txt");
        let original = b"original bytes that must survive";
        std::fs::write(&target, original).expect("write target");

        // Make the subdirectory read-only so CreateTemp fails.
        std::fs::set_permissions(&subdir, std::fs::Permissions::from_mode(0o555))
            .expect("chmod subdir");

        let rope = Rope::from_str("attempted overwrite\n");
        let err = save(&target, &rope, &utf8_opts(LineEnding::Lf)).unwrap_err();
        match err {
            SaveError::Io {
                step: SaveStep::CreateTemp,
                ..
            } => {}
            other => panic!("expected CreateTemp error, got {other:?}"),
        }

        // Restore permissions so the tempdir can be cleaned up.
        std::fs::set_permissions(&subdir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod subdir back");

        // Original content is unchanged.
        let on_disk = std::fs::read(&target).expect("read target");
        assert_eq!(&on_disk, original);
    }
}
