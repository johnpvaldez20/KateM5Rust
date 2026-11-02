// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! The convert script cites the pinned syntax-highlighting commit.

use std::fs;
use std::path::PathBuf;

const PINNED_COMMIT: &str = "6d8ab7945958e7354295db7f399cce06f7de7571";
const OLD_COMMIT: &str = "2d07614ca2ba8565915cc7e0959fd44745702ca1";
const PINNED_TAG: &str = "v6.31.0";

#[test]
fn convert_reference_cites_the_pinned_commit() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/convert_reference.py");
    let script = fs::read_to_string(&path).expect("convert_reference.py is readable");
    assert!(
        script.contains(PINNED_COMMIT),
        "script must cite {PINNED_COMMIT}"
    );
    assert!(
        !script.contains(OLD_COMMIT),
        "script must not cite {OLD_COMMIT}"
    );
    assert!(script.contains(PINNED_TAG), "script must cite {PINNED_TAG}");
}
