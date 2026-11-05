// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Read and write [`EditorConfig`] as TOML.
//!
//! The parser accepts `format_version`, `fallback_encoding`, and the
//! `document` fields. Each `document` field maps to the same field on
//! `default_document`.
//! A blank line is skipped. A line that starts with `#` after trim is a comment.
//! Spaces around `=` are accepted.
//! `load_editor_config` returns [`ConfigError::Io`] when the path is missing.
//! `save_editor_config` writes a sibling temp file and renames that file onto the path.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kate_fileio::Encoding;

use crate::{EditorConfig, TrailingSpaces};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// `load_editor_config` or `editor_config_from_toml` failed.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The file could not be read or written.
    Io { message: String },
    /// `format_version` is missing or is not 1.
    BadVersion { found: String },
    /// A known key has a value this parser rejects.
    BadValue { key: String },
    /// The key is not in the allowed set.
    UnknownKey { key: String },
}

/// Returns `home/Library/Application Support/KateM5Rust/config.toml`.
pub fn user_config_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("KateM5Rust")
        .join("config.toml")
}

/// Parses the TOML text into an [`EditorConfig`].
///
/// A missing `document` key keeps [`crate::DocumentConfig::default`] for that field.
/// `fallback_encoding` defaults to windows-1252.
pub fn editor_config_from_toml(text: &str) -> Result<EditorConfig, ConfigError> {
    let mut section: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut saw_version = false;
    let mut config = EditorConfig::default();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = Some(take_section(line, section.is_some())?);
            continue;
        }
        let (key, value) = assignment(line)?;
        let full = qualify(section.as_deref(), &key);
        if !seen.insert(full.clone()) {
            return Err(bad_value(&full));
        }
        apply(&mut config, &mut saw_version, &full, &value)?;
    }
    if !saw_version {
        return Err(ConfigError::BadVersion {
            found: String::new(),
        });
    }
    Ok(config)
}

/// Writes `config` as canonical TOML.
pub fn editor_config_to_toml(config: &EditorConfig) -> String {
    let doc = &config.default_document;
    let encoding = config.fallback_encoding.name();
    let trailing = trailing_word(doc.remove_trailing_spaces);
    format!(
        "\
format_version = 1
fallback_encoding = \"{encoding}\"
document.tab_width = {tab_width}
document.indent_width = {indent_width}
document.indent_with_spaces = {indent_with_spaces}
document.remove_trailing_spaces = \"{trailing}\"
document.ensure_trailing_newline = {ensure_trailing_newline}
",
        tab_width = doc.tab_width,
        indent_width = doc.indent_width,
        indent_with_spaces = doc.indent_with_spaces,
        ensure_trailing_newline = doc.ensure_trailing_newline,
    )
}

/// Reads `path` and parses it as editor config TOML.
pub fn load_editor_config(path: &Path) -> Result<EditorConfig, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(io_error)?;
    editor_config_from_toml(&text)
}

/// Writes canonical TOML to a sibling temp file, then renames it onto `path`.
pub fn save_editor_config(path: &Path, config: &EditorConfig) -> Result<(), ConfigError> {
    let text = editor_config_to_toml(config);
    let temp = sibling_temp(path)?;
    if let Err(err) = write_temp(&temp, &text) {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(io_error(err));
    }
    Ok(())
}

fn take_section(line: &str, already: bool) -> Result<String, ConfigError> {
    let name = section_name(line)?;
    if already {
        return Err(bad_value(&name));
    }
    if name != "document" {
        return Err(ConfigError::UnknownKey { key: name });
    }
    Ok(name)
}

fn section_name(line: &str) -> Result<String, ConfigError> {
    if line.contains('\'') {
        return Err(bad_value(line));
    }
    let Some(rest) = line.strip_prefix('[') else {
        return Err(bad_value(line));
    };
    let Some((name, after)) = rest.split_once(']') else {
        return Err(bad_value(line));
    };
    if !after.trim().is_empty() {
        return Err(bad_value(line));
    }
    let name = name.trim();
    if !is_key(name) {
        return Err(bad_value(name));
    }
    Ok(name.to_string())
}

fn assignment(line: &str) -> Result<(String, String), ConfigError> {
    let Some((raw_key, raw_value)) = line.split_once('=') else {
        return Err(bad_value(line));
    };
    let key = raw_key.trim();
    if !is_key(key) {
        return Err(bad_value(key));
    }
    let value = raw_rhs(key, raw_value)?;
    Ok((key.to_string(), value))
}

fn raw_rhs(key: &str, raw: &str) -> Result<String, ConfigError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains('\'') {
        return Err(bad_value(key));
    }
    if let Some(rest) = raw.strip_prefix('"') {
        return quoted_rhs(key, rest);
    }
    if raw.contains('#') || raw.chars().any(char::is_whitespace) {
        return Err(bad_value(key));
    }
    Ok(raw.to_string())
}

fn quoted_rhs(key: &str, rest: &str) -> Result<String, ConfigError> {
    let Some((inner, after)) = rest.split_once('"') else {
        return Err(bad_value(key));
    };
    if !after.trim().is_empty() || inner.contains('"') {
        return Err(bad_value(key));
    }
    Ok(format!("\"{inner}\""))
}

fn qualify(section: Option<&str>, key: &str) -> String {
    match section {
        Some(section) => format!("{section}.{key}"),
        None => key.to_string(),
    }
}

fn apply(
    config: &mut EditorConfig,
    saw_version: &mut bool,
    key: &str,
    value: &str,
) -> Result<(), ConfigError> {
    if let Some(field) = key.strip_prefix("document.") {
        return apply_document(config, field, value);
    }
    match key {
        "format_version" => set_version(saw_version, value),
        "fallback_encoding" => {
            config.fallback_encoding = encoding_value(value)?;
            Ok(())
        }
        _ => Err(ConfigError::UnknownKey {
            key: key.to_string(),
        }),
    }
}

fn apply_document(config: &mut EditorConfig, field: &str, value: &str) -> Result<(), ConfigError> {
    let doc = &mut config.default_document;
    match field {
        "tab_width" => doc.tab_width = width_value("document.tab_width", value)?,
        "indent_width" => doc.indent_width = width_value("document.indent_width", value)?,
        "indent_with_spaces" => {
            doc.indent_with_spaces = bool_value("document.indent_with_spaces", value)?;
        }
        "remove_trailing_spaces" => doc.remove_trailing_spaces = trailing_value(value)?,
        "ensure_trailing_newline" => {
            doc.ensure_trailing_newline = bool_value("document.ensure_trailing_newline", value)?;
        }
        _ => {
            return Err(ConfigError::UnknownKey {
                key: format!("document.{field}"),
            });
        }
    }
    Ok(())
}

fn set_version(saw_version: &mut bool, value: &str) -> Result<(), ConfigError> {
    if value != "1" {
        return Err(ConfigError::BadVersion {
            found: value.to_string(),
        });
    }
    *saw_version = true;
    Ok(())
}

fn encoding_value(value: &str) -> Result<Encoding, ConfigError> {
    let Some(label) = quoted(value) else {
        return Err(bad_value("fallback_encoding"));
    };
    encoding_for_label(label).ok_or_else(|| bad_value("fallback_encoding"))
}

/// kate-fileio owns the label table.
/// This function calls kate_fileio::encoding_for_label.
fn encoding_for_label(label: &str) -> Option<kate_fileio::Encoding> {
    kate_fileio::encoding_for_label(label)
}

fn width_value(key: &str, value: &str) -> Result<u8, ConfigError> {
    let Some(n) = parse_int(value) else {
        return Err(bad_value(key));
    };
    match u8::try_from(n) {
        Ok(width) if (1..=200).contains(&width) => Ok(width),
        _ => Err(bad_value(key)),
    }
}

fn bool_value(key: &str, value: &str) -> Result<bool, ConfigError> {
    let v = value.to_ascii_lowercase();
    match v.as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(bad_value(key)),
    }
}

fn trailing_value(value: &str) -> Result<TrailingSpaces, ConfigError> {
    let key = "document.remove_trailing_spaces";
    let Some(word) = quoted(value) else {
        return Err(bad_value(key));
    };
    match word {
        "never" => Ok(TrailingSpaces::Never),
        "modified" => Ok(TrailingSpaces::ModifiedLines),
        "all" => Ok(TrailingSpaces::All),
        _ => Err(bad_value(key)),
    }
}

fn trailing_word(value: TrailingSpaces) -> &'static str {
    match value {
        TrailingSpaces::Never => "never",
        TrailingSpaces::ModifiedLines => "modified",
        TrailingSpaces::All => "all",
    }
}

fn parse_int(token: &str) -> Option<u32> {
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if token.len() > 1 && token.starts_with('0') {
        return None;
    }
    token.parse().ok()
}

fn quoted(token: &str) -> Option<&str> {
    token.strip_prefix('"')?.strip_suffix('"')
}

fn is_key(key: &str) -> bool {
    let Some(first) = key.chars().next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !key.contains("..")
        && !key.ends_with('.')
}

fn bad_value(key: &str) -> ConfigError {
    ConfigError::BadValue {
        key: key.to_string(),
    }
}

fn io_error(err: std::io::Error) -> ConfigError {
    ConfigError::Io {
        message: err.to_string(),
    }
}

fn sibling_temp(path: &Path) -> Result<PathBuf, ConfigError> {
    let parent = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let Some(name) = path.file_name() else {
        return Err(ConfigError::Io {
            message: "path has no file name".to_string(),
        });
    };
    let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = format!(
        ".{}.katem5-{}-{n}",
        name.to_string_lossy(),
        std::process::id()
    );
    Ok(parent.join(file_name))
}

fn write_temp(path: &Path, text: &str) -> Result<(), ConfigError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(text.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        ConfigError, editor_config_from_toml, editor_config_to_toml, load_editor_config,
        save_editor_config, user_config_path,
    };
    use crate::{DocumentConfig, EditorConfig, TrailingSpaces};

    fn assert_same_config(left: &EditorConfig, right: &EditorConfig) {
        assert_eq!(left.fallback_encoding, right.fallback_encoding);
        assert_eq!(
            left.default_document.tab_width,
            right.default_document.tab_width
        );
        assert_eq!(
            left.default_document.indent_width,
            right.default_document.indent_width
        );
        assert_eq!(
            left.default_document.indent_with_spaces,
            right.default_document.indent_with_spaces
        );
        assert_eq!(
            left.default_document.remove_trailing_spaces,
            right.default_document.remove_trailing_spaces
        );
        assert_eq!(
            left.default_document.ensure_trailing_newline,
            right.default_document.ensure_trailing_newline
        );
    }

    #[test]
    fn toml_roundtrip_keeps_the_default() {
        let original = EditorConfig::default();
        let text = editor_config_to_toml(&original);
        assert!(
            text.starts_with("format_version = 1"),
            "toml starts with format_version = 1, got {text:?}"
        );
        let parsed = editor_config_from_toml(&text).expect("parse default toml");
        assert_same_config(&parsed, &original);
        assert_eq!(editor_config_to_toml(&original), text);
        assert_eq!(editor_config_to_toml(&parsed), text);
    }

    #[test]
    fn unknown_key_and_bad_version_fail() {
        let unknown = editor_config_from_toml("format_version = 1\nnot_a_key = 1\n")
            .expect_err("unknown key");
        assert_eq!(
            unknown,
            ConfigError::UnknownKey {
                key: "not_a_key".to_string(),
            }
        );

        let version = editor_config_from_toml("format_version = 2\n").expect_err("version");
        assert_eq!(
            version,
            ConfigError::BadVersion {
                found: "2".to_string(),
            }
        );

        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("no-such.toml");
        let io_error = load_editor_config(&missing).expect_err("missing file");
        assert!(matches!(io_error, ConfigError::Io { message } if !message.is_empty()));

        let width = editor_config_from_toml("format_version = 1\ndocument.tab_width = 0\n")
            .expect_err("width");
        assert_eq!(
            width,
            ConfigError::BadValue {
                key: "document.tab_width".to_string(),
            }
        );
    }

    #[test]
    fn user_config_path_uses_the_given_home() {
        let home = Path::new("/given/home");
        let path = user_config_path(home);
        let expected =
            PathBuf::from("/given/home/Library/Application Support/KateM5Rust/config.toml");
        assert_eq!(path, expected);
        assert!(path.starts_with(home));
        assert!(path.ends_with("Library/Application Support/KateM5Rust/config.toml"));
    }

    #[test]
    fn comments_blank_lines_and_spaces_around_equals_parse() {
        let text = "\r\n# don't keep this\r\n\n  format_version   =   1  \n\n";
        let config = editor_config_from_toml(text).expect("parse");
        assert_same_config(&config, &EditorConfig::default());
    }

    #[test]
    fn missing_document_key_keeps_the_field_default() {
        let text = "format_version = 1\ndocument.indent_with_spaces = false\n";
        let config = editor_config_from_toml(text).expect("parse");
        let defaults = DocumentConfig::default();
        assert!(!config.default_document.indent_with_spaces);
        assert_eq!(config.default_document.tab_width, defaults.tab_width);
        assert_eq!(config.default_document.indent_width, defaults.indent_width);
        assert_eq!(
            config.default_document.remove_trailing_spaces,
            defaults.remove_trailing_spaces
        );
        assert_eq!(
            config.default_document.ensure_trailing_newline,
            defaults.ensure_trailing_newline
        );
        assert_eq!(
            config.fallback_encoding,
            EditorConfig::default().fallback_encoding
        );
    }

    #[test]
    fn duplicate_key_fails() {
        let again = editor_config_from_toml("format_version = 1\nformat_version = 1\n")
            .expect_err("duplicate");
        assert_eq!(
            again,
            ConfigError::BadValue {
                key: "format_version".to_string(),
            }
        );

        let across = "format_version = 1\ndocument.tab_width = 4\n[document]\ntab_width = 8\n";
        let err = editor_config_from_toml(across).expect_err("duplicate field");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "document.tab_width".to_string(),
            }
        );
    }

    #[test]
    fn second_section_name_fails() {
        let text = "format_version = 1\n[document]\ntab_width = 4\n[document]\nindent_width = 4\n";
        let err = editor_config_from_toml(text).expect_err("second section");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "document".to_string(),
            }
        );
    }

    #[test]
    fn unknown_section_fails() {
        let err = editor_config_from_toml("format_version = 1\n[editor]\n").expect_err("section");
        assert_eq!(
            err,
            ConfigError::UnknownKey {
                key: "editor".to_string(),
            }
        );
    }

    #[test]
    fn inline_comment_fails() {
        let version = editor_config_from_toml("format_version = 1 # no\n").expect_err("comment");
        assert_eq!(
            version,
            ConfigError::BadValue {
                key: "format_version".to_string(),
            }
        );

        let quoted =
            editor_config_from_toml("format_version = 1\nfallback_encoding = \"UTF-8\" # note\n")
                .expect_err("comment after quotes");
        assert_eq!(
            quoted,
            ConfigError::BadValue {
                key: "fallback_encoding".to_string(),
            }
        );
    }

    #[test]
    fn single_quote_fails() {
        let text = "format_version = 1\nfallback_encoding = 'windows-1252'\n";
        let err = editor_config_from_toml(text).expect_err("single quote");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "fallback_encoding".to_string(),
            }
        );
    }

    #[test]
    fn document_section_maps_to_default_document() {
        let text = "\
format_version = 1
fallback_encoding = \"UTF-8\"

[document]
tab_width = 8
indent_width = 2
indent_with_spaces = false
remove_trailing_spaces = \"modified\"
ensure_trailing_newline = true
";
        let config = editor_config_from_toml(text).expect("parse section");
        assert_eq!(config.fallback_encoding.name(), "UTF-8");
        assert_eq!(config.default_document.tab_width, 8);
        assert_eq!(config.default_document.indent_width, 2);
        assert!(!config.default_document.indent_with_spaces);
        assert_eq!(
            config.default_document.remove_trailing_spaces,
            TrailingSpaces::ModifiedLines
        );
        assert!(config.default_document.ensure_trailing_newline);
    }

    #[test]
    fn changed_values_roundtrip() {
        let text = "\
format_version = 1
fallback_encoding = \"UTF-8\"
document.tab_width = 8
document.indent_width = 2
document.indent_with_spaces = false
document.remove_trailing_spaces = \"all\"
document.ensure_trailing_newline = true
";
        let config = editor_config_from_toml(text).expect("parse");
        let written = editor_config_to_toml(&config);
        assert!(written.starts_with("format_version = 1"));
        assert!(written.contains("fallback_encoding = \"UTF-8\""));
        assert!(written.contains("document.tab_width = 8"));
        assert!(written.contains("document.indent_width = 2"));
        assert!(written.contains("document.indent_with_spaces = false"));
        assert!(written.contains("document.remove_trailing_spaces = \"all\""));
        assert!(written.contains("document.ensure_trailing_newline = true"));
        let again = editor_config_from_toml(&written).expect("parse written");
        assert_same_config(&config, &again);
        assert_eq!(editor_config_to_toml(&again), written);
    }

    #[test]
    fn width_outside_1_to_200_fails() {
        let high = editor_config_from_toml("format_version = 1\ndocument.tab_width = 201\n")
            .expect_err("201");
        assert_eq!(
            high,
            ConfigError::BadValue {
                key: "document.tab_width".to_string(),
            }
        );
        let zero = editor_config_from_toml("format_version = 1\ndocument.indent_width = 0\n")
            .expect_err("0");
        assert_eq!(
            zero,
            ConfigError::BadValue {
                key: "document.indent_width".to_string(),
            }
        );
        let edges = editor_config_from_toml(
            "format_version = 1\ndocument.tab_width = 200\ndocument.indent_width = 1\n",
        )
        .expect("edges");
        assert_eq!(edges.default_document.tab_width, 200);
        assert_eq!(edges.default_document.indent_width, 1);
    }

    #[test]
    fn bad_encoding_label_fails() {
        let err = editor_config_from_toml(
            "format_version = 1\nfallback_encoding = \"not-a-real-encoding\"\n",
        )
        .expect_err("label");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "fallback_encoding".to_string(),
            }
        );
    }

    #[test]
    fn replacement_label_is_rejected() {
        let err =
            editor_config_from_toml("format_version = 1\nfallback_encoding = \"replacement\"\n")
                .expect_err("replacement");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "fallback_encoding".to_string(),
            }
        );

        let config = editor_config_from_toml("format_version = 1\nfallback_encoding = \"utf-8\"\n")
            .expect("utf-8");
        assert_eq!(config.fallback_encoding.name(), "UTF-8");
    }

    #[test]
    fn missing_format_version_fails() {
        let err = editor_config_from_toml("document.tab_width = 4\n").expect_err("version");
        assert_eq!(
            err,
            ConfigError::BadVersion {
                found: String::new(),
            }
        );
    }

    #[test]
    fn save_writes_a_sibling_temp_then_renames() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "old\n").expect("seed");
        let config = editor_config_from_toml(
            "format_version = 1\nfallback_encoding = \"UTF-8\"\ndocument.tab_width = 8\n",
        )
        .expect("parse");
        save_editor_config(&path, &config).expect("save");
        let on_disk = std::fs::read_to_string(&path).expect("read");
        assert_eq!(on_disk, editor_config_to_toml(&config));
        let loaded = load_editor_config(&path).expect("load");
        assert_same_config(&loaded, &config);
        let mut names = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec![std::ffi::OsString::from("config.toml")]);
    }

    #[test]
    fn save_missing_parent_returns_io() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("missing").join("config.toml");
        let err = save_editor_config(&path, &EditorConfig::default()).expect_err("save");
        assert!(matches!(err, ConfigError::Io { .. }));
    }

    #[test]
    fn yes_and_no_are_booleans() {
        let text = "format_version = 1\ndocument.indent_with_spaces = yes\ndocument.ensure_trailing_newline = no\n";
        let config = editor_config_from_toml(text).expect("yes and no parse");
        assert!(config.default_document.indent_with_spaces);
        assert!(!config.default_document.ensure_trailing_newline);

        let upper =
            editor_config_from_toml("format_version = 1\ndocument.indent_with_spaces = YES\n")
                .expect("uppercase YES parses as true");
        assert!(upper.default_document.indent_with_spaces);
    }

    #[test]
    fn on_and_off_are_booleans() {
        let text = "format_version = 1\ndocument.indent_with_spaces = on\ndocument.ensure_trailing_newline = off\n";
        let config = editor_config_from_toml(text).expect("on and off parse");
        assert!(config.default_document.indent_with_spaces);
        assert!(!config.default_document.ensure_trailing_newline);

        let upper =
            editor_config_from_toml("format_version = 1\ndocument.indent_with_spaces = ON\n")
                .expect("uppercase ON parses as true");
        assert!(upper.default_document.indent_with_spaces);
    }

    #[test]
    fn one_and_zero_are_booleans() {
        let text = "format_version = 1\ndocument.indent_with_spaces = 1\ndocument.ensure_trailing_newline = 0\n";
        let config = editor_config_from_toml(text).expect("1 and 0 parse");
        assert!(config.default_document.indent_with_spaces);
        assert!(!config.default_document.ensure_trailing_newline);

        let err = editor_config_from_toml("format_version = 1\ndocument.indent_with_spaces = 2\n")
            .expect_err("2 is bad");
        assert_eq!(
            err,
            ConfigError::BadValue {
                key: "document.indent_with_spaces".to_string(),
            }
        );
    }

    #[test]
    fn upper_case_booleans_parse() {
        let text = "format_version = 1\ndocument.indent_with_spaces = TRUE\ndocument.ensure_trailing_newline = FALSE\n";
        let config = editor_config_from_toml(text).expect("TRUE and FALSE parse");
        assert!(config.default_document.indent_with_spaces);
        assert!(!config.default_document.ensure_trailing_newline);
    }
}
