//! `kate-fileio`: files on disk.
//!
//! Encoding detection and conversion, byte-order marks, line-ending
//! normalization, atomic save (write to a temporary file, then rename), and
//! modified-on-disk detection. Depends on `kate-text` only.
//!
//! Specified in `docs/design/01-data-structures.md`, section "File I/O and
//! encodings". Arrives with packets PKT-0005 and PKT-0006.
