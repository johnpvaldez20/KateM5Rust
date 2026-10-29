// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo test --workspace` fails when any governance example or design packet
//! stops validating. This is the check CI relies on.

use std::path::PathBuf;

#[test]
fn all_examples_and_packets_validate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let report = gatecheck::run(&root).expect("gatecheck could not run");
    assert!(report.is_clean(), "gatecheck reported failures:\n{report}");
    assert!(!report.passed.is_empty(), "gatecheck checked nothing");
}
