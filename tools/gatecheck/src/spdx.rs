// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! SPDX header check for the license split in PKT-0043.
//!
//! The first matching prefix wins. More specific prefixes stay above broader
//! ones. A path with no match is an error for the caller, not a default.

use std::fs;
use std::path::{Path, PathBuf};

const COPYRIGHT_LINE: &str = "SPDX-FileCopyrightText: 2026 John Valdez";

const SPDX_BY_PREFIX: &[(&str, &str)] = &[
    ("crates/kate-syntax/", "MIT"),
    ("crates/kate-text/", "LGPL-2.1-or-later"),
    ("crates/kate-fileio/", "LGPL-2.1-or-later"),
    ("crates/kate-search/", "LGPL-2.1-or-later"),
    ("crates/kate-core/", "LGPL-2.1-or-later"),
    ("apps/", "GPL-3.0-or-later"),
    ("tools/", "GPL-3.0-or-later"),
];

/// License identifier required for a repo-relative path.
/// `None` means this packet does not map that path. The caller fails the check.
pub fn expected_spdx_license(rel_path: &str) -> Option<&'static str> {
    SPDX_BY_PREFIX
        .iter()
        .find(|(prefix, _)| rel_path.starts_with(prefix))
        .map(|(_, license)| *license)
}

/// Empty when every Rust file under `apps/`, `crates/`, and `tools/` starts
/// with the copyright line and the license line for its path.
pub fn check_rust_spdx_headers(repo_root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let mut files = Vec::new();
    for dir in ["apps", "crates", "tools"] {
        let path = repo_root.join(dir);
        if !path.is_dir() {
            errors.push(format!("missing directory {dir}"));
            continue;
        }
        collect_rs(&path, repo_root, &mut files, &mut errors);
    }
    files.sort();
    if files.is_empty() {
        errors.push("no Rust files found under apps/, crates/, or tools/".to_string());
    }
    for path in files {
        let rel = path.strip_prefix(repo_root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        match expected_spdx_license(&rel_str) {
            Some(license) => check_file(&path, &rel_str, license, &mut errors),
            None => errors.push(format!("{rel_str}: no license mapping for this path")),
        }
    }
    errors
}

fn check_file(path: &Path, rel: &str, license: &str, errors: &mut Vec<String>) {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            errors.push(format!("{rel}: {err}"));
            return;
        }
    };
    let head = text.lines().take(8).collect::<Vec<_>>().join("\n");
    if !head.contains(COPYRIGHT_LINE) {
        errors.push(format!("{rel}: missing {COPYRIGHT_LINE}"));
    }
    let license_line = format!("SPDX-License-Identifier: {license}");
    if !head.contains(&license_line) {
        errors.push(format!("{rel}: missing {license_line}"));
    }
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            let rel = dir.strip_prefix(root).unwrap_or(dir);
            errors.push(format!("{}: {err}", rel.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rs(&path, root, out, errors);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::expected_spdx_license;

    #[test]
    fn prefix_map_matches_the_owner_split() {
        assert_eq!(
            expected_spdx_license("crates/kate-syntax/src/lib.rs"),
            Some("MIT")
        );
        assert_eq!(
            expected_spdx_license("crates/kate-text/src/lib.rs"),
            Some("LGPL-2.1-or-later")
        );
        assert_eq!(
            expected_spdx_license("crates/kate-fileio/src/lib.rs"),
            Some("LGPL-2.1-or-later")
        );
        assert_eq!(
            expected_spdx_license("crates/kate-search/src/lib.rs"),
            Some("LGPL-2.1-or-later")
        );
        assert_eq!(
            expected_spdx_license("crates/kate-core/src/lib.rs"),
            Some("LGPL-2.1-or-later")
        );
        assert_eq!(
            expected_spdx_license("apps/katem5rust/src/main.rs"),
            Some("GPL-3.0-or-later")
        );
        assert_eq!(
            expected_spdx_license("tools/gatecheck/src/lib.rs"),
            Some("GPL-3.0-or-later")
        );
        assert_eq!(expected_spdx_license("docs/design/06-licensing.md"), None);
    }
}
