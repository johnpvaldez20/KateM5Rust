// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Read one path into a decoded rope plus a digest of the bytes on disk.

use std::fs::File;
use std::io::Read;
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

/// Open `path` with the non-blocking flag so a FIFO does not stall the caller.
#[cfg(unix)]
fn open_load_file(path: &Path) -> std::io::Result<File> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: i32 = 0x800;
    #[cfg(not(target_os = "linux"))]
    const O_NONBLOCK: i32 = 0x4;
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_load_file(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/// Reads `path` and decodes the bytes with `opts`.
/// The path must be a regular file.
/// The read stops one byte past `max_bytes`.
pub fn load(path: &Path, opts: &DecodeOptions) -> Result<Loaded, LoadError> {
    let file = open_load_file(path).map_err(LoadError::Io)?;
    let meta = file.metadata().map_err(LoadError::Io)?;
    if !meta.is_file() {
        return Err(LoadError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        )));
    }
    let mtime = meta.modified().ok();
    let mut bytes = Vec::new();
    file.take(opts.max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(LoadError::Io)?;
    let size = u64::try_from(bytes.len()).expect("byte length fits in u64");
    if size > opts.max_bytes {
        return Err(LoadError::Decode(DecodeError::TooLarge {
            size,
            limit: opts.max_bytes,
        }));
    }
    let decoded = decode(&bytes, opts).map_err(LoadError::Decode)?;
    Ok(Loaded {
        decoded,
        size_bytes: size,
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
    use super::{LoadError, load, open_load_file};
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

    #[test]
    fn load_returns_too_large_after_one_extra_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        std::fs::write(&path, vec![b'a'; 8]).unwrap();
        let opts = DecodeOptions {
            max_bytes: 4,
            ..DecodeOptions::default()
        };
        let err = load(&path, &opts).unwrap_err();
        assert!(matches!(
            err,
            super::LoadError::Decode(crate::decode::DecodeError::TooLarge { size: 5, limit: 4 })
        ));
    }

    #[test]
    #[cfg(unix)]
    fn a_fifo_returns_promptly() {
        use std::os::unix::fs::FileTypeExt;

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Seen {
            Io,
            Other,
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let fifo = dir.path().join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        assert!(made.success());
        let meta = std::fs::metadata(&fifo).expect("fifo meta");
        assert!(meta.file_type().is_fifo());
        assert!(!meta.is_file());
        let (send, recv) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = load(&fifo, &DecodeOptions::default());
            let seen = match result {
                Err(super::LoadError::Io(_)) => Seen::Io,
                _ => Seen::Other,
            };
            let _ = send.send(seen);
        });
        let seen = recv
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("load returns");
        worker.join().expect("worker");
        assert_eq!(seen, Seen::Io);
    }

    #[test]
    fn open_load_file_returns_a_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.txt");
        std::fs::write(&path, b"hello\n").unwrap();
        let file = open_load_file(&path).unwrap();
        assert!(file.metadata().unwrap().is_file());
        // A directory path makes load return LoadError::Io.
        let err = load(dir.path(), &DecodeOptions::default()).unwrap_err();
        assert!(matches!(err, LoadError::Io(_)));
    }

    #[test]
    #[cfg(unix)]
    fn a_fifo_load_open_returns_within_two_seconds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fifo = dir.path().join("pipe");
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        let (send, recv) = std::sync::mpsc::channel();
        let fifo_for_open = fifo.clone();
        let worker = std::thread::spawn(move || {
            open_load_file(&fifo_for_open).unwrap();
            let _ = send.send(());
        });
        recv.recv_timeout(std::time::Duration::from_secs(2))
            .expect("open_load_file returns within 2 s");
        worker.join().expect("worker");
        let err = load(&fifo, &DecodeOptions::default()).unwrap_err();
        assert!(matches!(err, LoadError::Io(_)));
    }

    fn sha256(bytes: &[u8]) -> [u8; 32] {
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(digest.as_slice());
        out
    }
}
