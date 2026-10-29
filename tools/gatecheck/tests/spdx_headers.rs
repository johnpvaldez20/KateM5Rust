// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fails the build when a Rust file lacks the SPDX header for its crate.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn every_rust_file_has_the_header_for_its_crate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let errors = gatecheck::check_rust_spdx_headers(&root);
    assert!(
        errors.is_empty(),
        "SPDX header check failed:\n{}",
        errors.join("\n")
    );
    assert_license_field(&root.join("Cargo.toml"), "GPL-3.0-or-later");
    assert_license_field(&root.join("apps/katem5rust/Cargo.toml"), "GPL-3.0-or-later");
    assert_license_field(&root.join("tools/gatecheck/Cargo.toml"), "GPL-3.0-or-later");
    for crate_name in ["kate-text", "kate-fileio", "kate-search", "kate-core"] {
        let path = root.join("crates").join(crate_name).join("Cargo.toml");
        assert_license_field(&path, "LGPL-2.1-or-later");
    }
    assert_license_field(&root.join("crates/kate-syntax/Cargo.toml"), "MIT");
}

fn assert_license_field(path: &Path, expected: &str) {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let needle = format!("license = \"{expected}\"");
    assert!(
        text.lines().any(|line| line.trim() == needle),
        "{} does not set {needle}",
        path.display()
    );
}
