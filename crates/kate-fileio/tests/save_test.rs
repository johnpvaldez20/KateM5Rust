// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Integration test: encode and save round-trip through the filesystem.

use kate_fileio::{decode, save, DecodeOptions, EncodeOptions, LineEnding};
use ropey::Rope;
use tempfile::tempdir;

#[test]
fn save_and_read_back_matches_encoded_bytes() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("hello.txt");

    let text = "Hello, World!\n";
    let rope = Rope::from_str(text);
    let opts = EncodeOptions {
        encoding: encoding_rs::UTF_8,
        bom: false,
        line_ending: LineEnding::Lf,
        ensure_trailing_newline: false,
    };

    let saved = save(&path, &rope, &opts).expect("save should succeed");
    assert_eq!(saved.bytes_written, text.len() as u64);

    let on_disk = std::fs::read(&path).expect("read file");
    assert_eq!(on_disk, text.as_bytes());

    // Decode round-trip.
    let dec_opts = DecodeOptions {
        max_bytes: 10_000,
        ..DecodeOptions::default()
    };
    let decoded = decode(&on_disk, &dec_opts).expect("decode should succeed");
    assert_eq!(decoded.text.to_string(), text);
}
