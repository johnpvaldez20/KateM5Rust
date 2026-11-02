// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! KateM5Rust application entry point.
//!
//! `katem5rust` opens one document.
//! `katem5rust --generate-fixture scroll.rs` writes the 100,000-line scroll file.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next() {
        Some(arg) if arg == "--generate-fixture" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("scroll_100k.rs"));
            if let Err(err) = kate_ui::write_rust_fixture(&path) {
                eprintln!("Cannot write the fixture. {err}");
                std::process::exit(1);
            }
        }
        Some(path) => kate_ui::run(Some(PathBuf::from(path))),
        None => kate_ui::run(None),
    }
}
