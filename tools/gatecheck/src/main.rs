// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Command-line entry point. Usage: `cargo run -p gatecheck [-- <repo-root>]`.
//! Defaults to the workspace root. Exit code 1 on any failure.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(gatecheck_workspace_root);
    match gatecheck::run(&root) {
        Ok(report) => {
            print!("{report}");
            if report.is_clean() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("gatecheck: {e}");
            ExitCode::FAILURE
        }
    }
}

fn gatecheck_workspace_root() -> PathBuf {
    // tools/gatecheck -> workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}
