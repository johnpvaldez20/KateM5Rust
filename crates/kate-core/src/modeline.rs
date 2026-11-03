// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Kate modelines and EditorConfig overrides for one open file.
//!
//! Open reads `.editorconfig` first.
//! A modeline then overrides each field it sets.

use std::path::{Path, PathBuf};

use kate_fileio::LineEnding;

use crate::{DocumentConfig, FileState, TrailingSpaces};

/// Fields a modeline or an EditorConfig file sets.
///
/// `None` means that source does not set the field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeOverride {
    /// Tab stop width when the source sets one.
    pub tab_width: Option<u8>,
    /// Indent step width when the source sets one.
    pub indent_width: Option<u8>,
    /// `Some(true)` when an indent step uses spaces.
    pub indent_with_spaces: Option<bool>,
    /// Trailing-space policy when the source sets one.
    pub remove_trailing_spaces: Option<crate::TrailingSpaces>,
    /// Final newline policy when the source sets one.
    pub ensure_trailing_newline: Option<bool>,
    /// Line ending written on save when the source sets one.
    pub line_ending: Option<kate_fileio::LineEnding>,
    /// Encoding label for a later write.
    ///
    /// [`apply_mode`] stores a known label as the file encoding.
    /// An unknown label does not change the file.
    pub encoding_label: Option<String>,
}

const EDGE_LINES: usize = 10;

/// Returns modeline fields from the first 10 lines and the last 10 lines.
///
/// Each selected line is read once, in file order.
/// A line matches when it contains `kate:` and the byte before it is absent or not an ASCII letter or digit.
pub fn modeline_overrides(text: &str) -> ModeOverride {
    let mut over = ModeOverride::default();
    for line in modeline_lines(text) {
        if let Some(body) = modeline_body(line) {
            apply_modeline_body(body, &mut over);
        }
    }
    over
}

/// Returns EditorConfig fields that apply to `file_name`.
///
/// Keys before a section apply like `*`.
/// The sections `*` and `*.ext` apply. Other sections are ignored.
/// A comment line starts with `#` or `;`.
pub fn editorconfig_overrides(text: &str, file_name: &str) -> ModeOverride {
    let mut over = ModeOverride::default();
    let mut applies = true;
    for line in text.lines() {
        if let Some(name) = section_name(line) {
            applies = section_matches(name, file_name);
            continue;
        }
        if applies && let Some((key, value)) = property(line) {
            set_editorconfig_key(&mut over, key, value);
        }
    }
    over
}

/// Largest `.editorconfig` file `read_editorconfig` reads, in bytes.
///
/// A larger file is skipped. A skipped file does not set root.
pub const MAX_EDITORCONFIG_BYTES: u64 = 1_048_576;

/// Reads `.editorconfig` from the file directory through its parents.
///
/// The nearest file wins when both set one field.
/// `root = true` stops the walk after that file.
/// A path that is not a file is skipped.
/// A file larger than [`MAX_EDITORCONFIG_BYTES`] is skipped.
/// A skipped path does not set root.
pub fn read_editorconfig(start: &Path) -> ModeOverride {
    let Some(file_name) = start.file_name().and_then(|name| name.to_str()) else {
        return ModeOverride::default();
    };
    let Some(mut dir) = directory_of(start) else {
        return ModeOverride::default();
    };
    let mut layers = Vec::new();
    loop {
        let stop = push_layer(&dir, file_name, &mut layers);
        if stop || !dir.pop() {
            break;
        }
    }
    merge_layers(&layers)
}

/// Copies each set field onto `config` and `file`.
///
/// A known encoding label replaces the encoding used for a later write.
/// It does not change the byte-order mark, the line ending, or the digest.
/// An unknown label leaves the file encoding unchanged.
pub fn apply_mode(config: &mut DocumentConfig, file: &mut FileState, over: &ModeOverride) {
    if let Some(value) = over.tab_width {
        config.tab_width = value;
    }
    if let Some(value) = over.indent_width {
        config.indent_width = value;
    }
    if let Some(value) = over.indent_with_spaces {
        config.indent_with_spaces = value;
    }
    if let Some(value) = over.remove_trailing_spaces {
        config.remove_trailing_spaces = value;
    }
    if let Some(value) = over.ensure_trailing_newline {
        config.ensure_trailing_newline = value;
    }
    if let Some(value) = over.line_ending {
        file.line_ending = value;
    }
    if let Some(label) = &over.encoding_label {
        apply_write_encoding(file, label);
    }
}

fn apply_write_encoding(file: &mut FileState, label: &str) {
    if let Some(encoding) = write_encoding(label) {
        file.encoding = encoding;
    }
}

/// Encoding Standard labels for UTF-8, UTF-16, and windows-1252.
///
/// Those are the encodings this crate can name. Any other label is unknown.
#[derive(Clone, Copy)]
enum KnownEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

const LABELS: &[(&str, KnownEncoding)] = &[
    ("ansi_x3.4-1968", KnownEncoding::Windows1252),
    ("ascii", KnownEncoding::Windows1252),
    ("cp1252", KnownEncoding::Windows1252),
    ("cp819", KnownEncoding::Windows1252),
    ("csisolatin1", KnownEncoding::Windows1252),
    ("csunicode", KnownEncoding::Utf16Le),
    ("ibm819", KnownEncoding::Windows1252),
    ("iso-10646-ucs-2", KnownEncoding::Utf16Le),
    ("iso-8859-1", KnownEncoding::Windows1252),
    ("iso-ir-100", KnownEncoding::Windows1252),
    ("iso8859-1", KnownEncoding::Windows1252),
    ("iso88591", KnownEncoding::Windows1252),
    ("iso_8859-1", KnownEncoding::Windows1252),
    ("iso_8859-1:1987", KnownEncoding::Windows1252),
    ("l1", KnownEncoding::Windows1252),
    ("latin1", KnownEncoding::Windows1252),
    ("ucs-2", KnownEncoding::Utf16Le),
    ("unicode", KnownEncoding::Utf16Le),
    ("unicode-1-1-utf-8", KnownEncoding::Utf8),
    ("unicode11utf8", KnownEncoding::Utf8),
    ("unicode20utf8", KnownEncoding::Utf8),
    ("unicodefeff", KnownEncoding::Utf16Le),
    ("unicodefffe", KnownEncoding::Utf16Be),
    ("us-ascii", KnownEncoding::Windows1252),
    ("utf-16", KnownEncoding::Utf16Le),
    ("utf-16be", KnownEncoding::Utf16Be),
    ("utf-16le", KnownEncoding::Utf16Le),
    ("utf-8", KnownEncoding::Utf8),
    ("utf8", KnownEncoding::Utf8),
    ("windows-1252", KnownEncoding::Windows1252),
    ("x-cp1252", KnownEncoding::Windows1252),
    ("x-unicode20utf8", KnownEncoding::Utf8),
];

fn write_encoding(label: &str) -> Option<kate_fileio::Encoding> {
    let normalized = normalize_label(label)?;
    let index = LABELS
        .binary_search_by_key(&normalized.as_str(), |row| row.0)
        .ok()?;
    known_encoding(LABELS[index].1)
}

fn normalize_label(label: &str) -> Option<String> {
    let trimmed = label.trim_matches(is_label_space);
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if !push_label_char(&mut out, ch) {
            return None;
        }
    }
    Some(out)
}

fn is_label_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}

fn push_label_char(out: &mut String, ch: char) -> bool {
    if ch.is_ascii_uppercase() {
        out.push(ch.to_ascii_lowercase());
        return true;
    }
    if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_' | ':' | '.') {
        out.push(ch);
        return true;
    }
    false
}

fn known_encoding(kind: KnownEncoding) -> Option<kate_fileio::Encoding> {
    let opts = kate_fileio::DecodeOptions::default();
    let bytes: &[u8] = match kind {
        KnownEncoding::Utf8 => b"a",
        KnownEncoding::Utf16Le => b"\xFF\xFEa\x00",
        KnownEncoding::Utf16Be => b"\xFE\xFF\x00a",
        KnownEncoding::Windows1252 => return Some(opts.fallback),
    };
    kate_fileio::decode(bytes, &opts)
        .ok()
        .map(|decoded| decoded.encoding)
}

fn modeline_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let count = lines.len();
    let head_end = count.min(EDGE_LINES);
    let second = count.saturating_sub(EDGE_LINES).max(head_end);
    let mut out = Vec::with_capacity(head_end + count.saturating_sub(second));
    out.extend(lines.iter().take(head_end).copied());
    out.extend(lines.iter().skip(second).copied());
    out
}

fn modeline_body(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find("kate:") {
        let at = from + rel;
        let boundary = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        if boundary {
            return Some(&line[at + 5..]);
        }
        from = at + 1;
    }
    None
}

fn apply_modeline_body(body: &str, over: &mut ModeOverride) {
    for part in body.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut words = part.split_whitespace();
        let Some(key) = words.next() else {
            continue;
        };
        let Some(value) = words.next() else {
            continue;
        };
        set_modeline_key(over, key, value);
    }
}

fn set_modeline_key(over: &mut ModeOverride, key: &str, value: &str) {
    if set_modeline_width(over, key, value) || set_modeline_flag(over, key, value) {
        return;
    }
    set_modeline_rest(over, key, value);
}

fn set_modeline_width(over: &mut ModeOverride, key: &str, value: &str) -> bool {
    match key {
        "indent-width" => set_width(&mut over.indent_width, value),
        "tab-width" => set_width(&mut over.tab_width, value),
        _ => return false,
    }
    true
}

fn set_modeline_flag(over: &mut ModeOverride, key: &str, value: &str) -> bool {
    let slot = match key {
        "replace-tabs" | "space-indent" => &mut over.indent_with_spaces,
        "newline-at-eof" => &mut over.ensure_trailing_newline,
        _ => return false,
    };
    if let Some(flag) = kate_bool(value) {
        *slot = Some(flag);
    }
    true
}

fn set_modeline_rest(over: &mut ModeOverride, key: &str, value: &str) {
    match key {
        "remove-trailing-spaces" => {
            if let Some(policy) = trailing_policy(value) {
                over.remove_trailing_spaces = Some(policy);
            }
        }
        "eol" | "end-of-line" => {
            if let Some(ending) = modeline_ending(value) {
                over.line_ending = Some(ending);
            }
        }
        "encoding" => over.encoding_label = Some(value.to_string()),
        _ => {}
    }
}

fn set_width(slot: &mut Option<u8>, value: &str) {
    if let Some(width) = width_in_range(value) {
        *slot = Some(width);
    }
}

fn width_in_range(value: &str) -> Option<u8> {
    let parsed = value.parse::<u16>().ok()?;
    if (1..=200).contains(&parsed) {
        u8::try_from(parsed).ok()
    } else {
        None
    }
}

fn kate_bool(value: &str) -> Option<bool> {
    match value {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn trailing_policy(value: &str) -> Option<TrailingSpaces> {
    match value {
        "none" | "0" | "-" => Some(TrailingSpaces::Never),
        "modified" | "mod" | "+" | "1" => Some(TrailingSpaces::ModifiedLines),
        "all" | "*" | "2" => Some(TrailingSpaces::All),
        _ => None,
    }
}

fn modeline_ending(value: &str) -> Option<LineEnding> {
    match value {
        "unix" => Some(LineEnding::Lf),
        "dos" => Some(LineEnding::CrLf),
        "mac" => Some(LineEnding::Cr),
        _ => None,
    }
}

fn section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.trim())
}

fn section_matches(name: &str, file_name: &str) -> bool {
    if name == "*" {
        return true;
    }
    let Some(ext) = name.strip_prefix("*.") else {
        return false;
    };
    if !plain_extension(ext) {
        return false;
    }
    let mut suffix = String::with_capacity(ext.len() + 1);
    suffix.push('.');
    suffix.push_str(ext);
    file_name.ends_with(&suffix)
}

fn plain_extension(ext: &str) -> bool {
    !ext.is_empty()
        && !ext.bytes().any(|byte| {
            matches!(
                byte,
                b'*' | b'?' | b'[' | b']' | b'{' | b'}' | b'/' | b'\\' | b' ' | b'\t'
            )
        })
}

fn property(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with(';')
        || trimmed.starts_with('[')
    {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim()))
}

fn set_editorconfig_key(over: &mut ModeOverride, key: &str, value: &str) {
    if set_editorconfig_indent(over, key, value) {
        return;
    }
    set_editorconfig_save(over, key, value);
}

fn set_editorconfig_indent(over: &mut ModeOverride, key: &str, value: &str) -> bool {
    if key.eq_ignore_ascii_case("indent_size") {
        set_indent_size(over, value);
    } else if key.eq_ignore_ascii_case("tab_width") {
        set_width(&mut over.tab_width, value);
    } else if key.eq_ignore_ascii_case("indent_style") {
        if let Some(flag) = indent_style(value) {
            over.indent_with_spaces = Some(flag);
        }
    } else {
        return false;
    }
    true
}

fn set_editorconfig_save(over: &mut ModeOverride, key: &str, value: &str) {
    if key.eq_ignore_ascii_case("trim_trailing_whitespace") {
        if let Some(policy) = trim_policy(value) {
            over.remove_trailing_spaces = Some(policy);
        }
    } else if key.eq_ignore_ascii_case("insert_final_newline") {
        if let Some(flag) = ec_bool(value) {
            over.ensure_trailing_newline = Some(flag);
        }
    } else if key.eq_ignore_ascii_case("end_of_line") {
        if let Some(ending) = ec_ending(value) {
            over.line_ending = Some(ending);
        }
    } else if key.eq_ignore_ascii_case("charset") {
        over.encoding_label = Some(value.to_string());
    }
}

fn set_indent_size(over: &mut ModeOverride, value: &str) {
    if value.eq_ignore_ascii_case("tab") {
        return;
    }
    set_width(&mut over.indent_width, value);
}

fn indent_style(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("space") {
        Some(true)
    } else if value.eq_ignore_ascii_case("tab") {
        Some(false)
    } else {
        None
    }
}

fn trim_policy(value: &str) -> Option<TrailingSpaces> {
    if value.eq_ignore_ascii_case("true") {
        Some(TrailingSpaces::All)
    } else if value.eq_ignore_ascii_case("false") {
        Some(TrailingSpaces::Never)
    } else {
        None
    }
}

fn ec_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn ec_ending(value: &str) -> Option<LineEnding> {
    if value.eq_ignore_ascii_case("lf") {
        Some(LineEnding::Lf)
    } else if value.eq_ignore_ascii_case("crlf") {
        Some(LineEnding::CrLf)
    } else if value.eq_ignore_ascii_case("cr") {
        Some(LineEnding::Cr)
    } else {
        None
    }
}

fn directory_of(start: &Path) -> Option<PathBuf> {
    let parent = start.parent()?;
    if parent.as_os_str().is_empty() {
        Some(PathBuf::from("."))
    } else {
        Some(parent.to_path_buf())
    }
}

fn read_capped(file: &mut std::fs::File) -> std::io::Result<Option<String>> {
    use std::io::Read;

    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > MAX_EDITORCONFIG_BYTES {
        return Ok(None);
    }
    let mut text = String::new();
    file.take(MAX_EDITORCONFIG_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_EDITORCONFIG_BYTES {
        return Ok(None);
    }
    Ok(Some(text))
}

fn push_layer(dir: &Path, file_name: &str, layers: &mut Vec<ModeOverride>) -> bool {
    let path = dir.join(".editorconfig");
    let Ok(meta) = std::fs::metadata(&path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(&path) else {
        return false;
    };
    let Ok(Some(text)) = read_capped(&mut file) else {
        return false;
    };
    let root = text_is_root(&text);
    layers.push(editorconfig_overrides(&text, file_name));
    root
}

fn text_is_root(text: &str) -> bool {
    text.lines().any(|line| {
        property(line).is_some_and(|(key, value)| {
            key.eq_ignore_ascii_case("root") && value.eq_ignore_ascii_case("true")
        })
    })
}

fn merge_layers(layers: &[ModeOverride]) -> ModeOverride {
    let mut merged = ModeOverride::default();
    for layer in layers.iter().rev() {
        copy_set(&mut merged, layer);
    }
    merged
}

fn copy_set(dst: &mut ModeOverride, src: &ModeOverride) {
    copy_copy(&mut dst.tab_width, src.tab_width);
    copy_copy(&mut dst.indent_width, src.indent_width);
    copy_copy(&mut dst.indent_with_spaces, src.indent_with_spaces);
    copy_copy(&mut dst.remove_trailing_spaces, src.remove_trailing_spaces);
    copy_copy(
        &mut dst.ensure_trailing_newline,
        src.ensure_trailing_newline,
    );
    copy_copy(&mut dst.line_ending, src.line_ending);
    if let Some(label) = &src.encoding_label {
        dst.encoding_label = Some(label.clone());
    }
}

fn copy_copy<T: Copy>(dst: &mut Option<T>, src: Option<T>) {
    if let Some(value) = src {
        *dst = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kate_fileio::{DecodeOptions, LineEnding};

    use super::{
        DocumentConfig, FileState, MAX_EDITORCONFIG_BYTES, TrailingSpaces, apply_mode,
        editorconfig_overrides, modeline_overrides, read_capped, read_editorconfig,
    };

    /// One byte past [`MAX_EDITORCONFIG_BYTES`].
    const OVERSIZED_EDITORCONFIG_LEN: u64 = MAX_EDITORCONFIG_BYTES + 1;

    fn oversized_editorconfig_body() -> Vec<u8> {
        let mut body = b"root = true\n[*]\nindent_size = 2\n".to_vec();
        body.push(b'\n');
        let target = usize::try_from(OVERSIZED_EDITORCONFIG_LEN).expect("limit");
        body.resize(target, b'#');
        assert_eq!(body.len(), target);
        body
    }

    fn sample_file(ending: LineEnding) -> FileState {
        FileState {
            path: PathBuf::from("sample.txt"),
            encoding: DecodeOptions::default().fallback,
            bom: false,
            line_ending: ending,
            digest_on_disk: [0; 32],
            mtime_on_disk: None,
            disk_change: None,
        }
    }

    #[test]
    fn modeline_wins_over_editorconfig() {
        let editor = editorconfig_overrides("[*]\nindent_size = 8\n", "lib.rs");
        let mode = modeline_overrides("// kate: indent-width 2; replace-tabs on; eol dos;\n");
        let mut config = DocumentConfig {
            indent_with_spaces: false,
            ..DocumentConfig::default()
        };
        let mut file = sample_file(LineEnding::Lf);
        apply_mode(&mut config, &mut file, &editor);
        assert_eq!(config.indent_width, 8);
        apply_mode(&mut config, &mut file, &mode);
        assert_eq!(config.indent_width, 2);
        assert!(config.indent_with_spaces);
        assert_eq!(file.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn unknown_encoding_label_is_ignored() {
        let mut config = DocumentConfig {
            indent_width: 5,
            ..DocumentConfig::default()
        };
        let mut file = sample_file(LineEnding::Lf);
        let original = file.encoding;
        let ignored = modeline_overrides("kate: encoding nope; indent-width 0;\n");
        apply_mode(&mut config, &mut file, &ignored);
        assert_eq!(file.encoding, original);
        assert_eq!(config.indent_width, 5);
        let utf8 = modeline_overrides("kate: encoding utf-8;\n");
        apply_mode(&mut config, &mut file, &utf8);
        assert_eq!(file.encoding.name(), "UTF-8");
        assert!(!file.bom);
        assert_eq!(file.line_ending, LineEnding::Lf);
        assert_eq!(file.digest_on_disk, [0; 32]);
        assert!(file.mtime_on_disk.is_none());
        assert!(file.disk_change.is_none());
        assert_eq!(config.indent_width, 5);
    }

    #[test]
    fn encoding_label_changes_only_the_write_encoding() {
        let utf8 = kate_fileio::decode(b"a", &DecodeOptions::default())
            .expect("utf-8")
            .encoding;
        let mut config = DocumentConfig::default();
        let mut file = sample_file(LineEnding::Cr);
        file.encoding = utf8;
        file.bom = true;
        let mode = modeline_overrides("kate: encoding iso-8859-1;\n");
        apply_mode(&mut config, &mut file, &mode);
        assert_eq!(file.encoding.name(), "windows-1252");
        assert!(file.bom);
        assert_eq!(file.line_ending, LineEnding::Cr);
        assert_eq!(config, DocumentConfig::default());
        let utf16 = modeline_overrides("kate: encoding UTF-16LE;\n");
        apply_mode(&mut config, &mut file, &utf16);
        assert_eq!(file.encoding.name(), "UTF-16LE");
        assert!(file.bom);
        let utf16be = modeline_overrides("kate: encoding utf-16be;\n");
        apply_mode(&mut config, &mut file, &utf16be);
        assert_eq!(file.encoding.name(), "UTF-16BE");
        assert_eq!(file.line_ending, LineEnding::Cr);
    }

    #[test]
    fn modeline_maps_the_documented_values() {
        let mode = modeline_overrides(
            "kate: indent-width 201; tab-width 200; space-indent off; newline-at-eof 0; remove-trailing-spaces mod; end-of-line mac; mystery 1; indent-width 1;\n",
        );
        assert_eq!(mode.indent_width, Some(1));
        assert_eq!(mode.tab_width, Some(200));
        assert_eq!(mode.indent_with_spaces, Some(false));
        assert_eq!(mode.ensure_trailing_newline, Some(false));
        assert_eq!(
            mode.remove_trailing_spaces,
            Some(TrailingSpaces::ModifiedLines)
        );
        assert_eq!(mode.line_ending, Some(LineEnding::Cr));
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces all;\n").remove_trailing_spaces,
            Some(TrailingSpaces::All)
        );
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces *;\n").remove_trailing_spaces,
            Some(TrailingSpaces::All)
        );
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces 2;\n").remove_trailing_spaces,
            Some(TrailingSpaces::All)
        );
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces -;\n").remove_trailing_spaces,
            Some(TrailingSpaces::Never)
        );
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces none;\n").remove_trailing_spaces,
            Some(TrailingSpaces::Never)
        );
        assert_eq!(
            modeline_overrides("kate: remove-trailing-spaces +;\n").remove_trailing_spaces,
            Some(TrailingSpaces::ModifiedLines)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs 1; newline-at-eof true;\n").indent_with_spaces,
            Some(true)
        );
    }

    #[test]
    fn yes_and_no_set_a_modeline_flag() {
        assert_eq!(
            modeline_overrides("kate: replace-tabs yes;\n").indent_with_spaces,
            Some(true)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs no;\n").indent_with_spaces,
            Some(false)
        );
        assert_eq!(
            modeline_overrides("kate: space-indent yes;\n").indent_with_spaces,
            Some(true)
        );
        assert_eq!(
            modeline_overrides("kate: newline-at-eof no;\n").ensure_trailing_newline,
            Some(false)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs on;\n").indent_with_spaces,
            Some(true)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs off;\n").indent_with_spaces,
            Some(false)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs true;\n").indent_with_spaces,
            Some(true)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs false;\n").indent_with_spaces,
            Some(false)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs 1;\n").indent_with_spaces,
            Some(true)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs 0;\n").indent_with_spaces,
            Some(false)
        );
        assert_eq!(
            modeline_overrides("kate: replace-tabs maybe;\n").indent_with_spaces,
            None
        );
    }

    #[test]
    fn modeline_uses_each_edge_line_once_in_file_order() {
        let mut lines = Vec::new();
        lines.push("// kate: indent-width 2;".to_string());
        lines.push("xkate: remove-trailing-spaces all;".to_string());
        for index in 2..24 {
            if index == 12 {
                lines.push("// kate: newline-at-eof on;".to_string());
            } else if index == 16 {
                lines.push("//_kate: space-indent on;".to_string());
            } else {
                lines.push(format!("line {index}"));
            }
        }
        lines.push("// kate: indent-width 4; tab-width 3;".to_string());
        assert_eq!(lines.len(), 25);
        let mode = modeline_overrides(&lines.join("\n"));
        assert_eq!(mode.indent_width, Some(4));
        assert_eq!(mode.tab_width, Some(3));
        assert_eq!(mode.indent_with_spaces, Some(true));
        assert_eq!(mode.ensure_trailing_newline, None);
        assert_eq!(mode.remove_trailing_spaces, None);
    }

    #[test]
    fn editorconfig_matches_star_extension_and_preamble() {
        let text = "\
# comment
; also a comment
indent_size = 4
tab_width = 2
indent_size = tab

[*.rs]
indent_size = 8
indent_style = tab

[lib.rs]
tab_width = 9

[*.c]
indent_size = 3
";
        let over = editorconfig_overrides(text, "foo.bar.rs");
        assert_eq!(over.indent_width, Some(8));
        assert_eq!(over.indent_with_spaces, Some(false));
        assert_eq!(over.tab_width, Some(2));
        let preamble = editorconfig_overrides("indent_size = tab\nindent_size = 0\n", "a.txt");
        assert_eq!(preamble.indent_width, None);
        let flags = editorconfig_overrides(
            "trim_trailing_whitespace = false\ninsert_final_newline = true\nend_of_line = crlf\ncharset = utf-8\nindent_style = space\n",
            "a.txt",
        );
        assert_eq!(flags.remove_trailing_spaces, Some(TrailingSpaces::Never));
        assert_eq!(flags.ensure_trailing_newline, Some(true));
        assert_eq!(flags.line_ending, Some(LineEnding::CrLf));
        assert_eq!(flags.encoding_label.as_deref(), Some("utf-8"));
        assert_eq!(flags.indent_with_spaces, Some(true));
    }

    #[test]
    fn read_editorconfig_stops_at_root_and_lets_the_near_file_win() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(
            parent.join(".editorconfig"),
            "[*]\nindent_size = 8\ntab_width = 5\n",
        )
        .expect("parent config");
        std::fs::write(
            child.join(".editorconfig"),
            "root = true\n[*]\nindent_size = 2\n",
        )
        .expect("child config");
        let stopped = read_editorconfig(&child.join("lib.rs"));
        assert_eq!(stopped.indent_width, Some(2));
        assert_eq!(stopped.tab_width, None);

        std::fs::write(child.join(".editorconfig"), "[*]\nindent_size = 2\n").expect("open root");
        let merged = read_editorconfig(&child.join("lib.rs"));
        assert_eq!(merged.indent_width, Some(2));
        assert_eq!(merged.tab_width, Some(5));
    }

    #[test]
    fn a_directory_named_editorconfig_is_skipped() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(
            parent.join(".editorconfig"),
            "[*]\nindent_size = 8\ntab_width = 4\n",
        )
        .expect("parent config");
        std::fs::create_dir(child.join(".editorconfig")).expect("config dir");
        let over = read_editorconfig(&child.join("lib.rs"));
        assert_eq!(over.indent_width, Some(8));
        assert_eq!(over.tab_width, Some(4));
    }

    #[test]
    fn an_oversized_editorconfig_is_skipped() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(
            parent.join(".editorconfig"),
            "[*]\nindent_size = 8\ntab_width = 4\n",
        )
        .expect("parent config");
        let path = child.join(".editorconfig");
        std::fs::write(&path, oversized_editorconfig_body()).expect("big config");
        let len = std::fs::metadata(&path).expect("meta").len();
        assert_eq!(len, OVERSIZED_EDITORCONFIG_LEN);
        let over = read_editorconfig(&child.join("lib.rs"));
        assert_eq!(over.indent_width, Some(8));
        assert_eq!(over.tab_width, Some(4));
    }

    #[test]
    fn read_capped_stops_past_the_cap() {
        let root = tempfile::tempdir().expect("tempdir");
        let small_path = root.path().join("small");
        let text = "root = true\n";
        std::fs::write(&small_path, text).expect("small");
        let mut small = std::fs::File::open(&small_path).expect("open small");
        let got = read_capped(&mut small).expect("read small");
        assert_eq!(got.as_deref(), Some(text));

        let big_path = root.path().join("big");
        std::fs::write(&big_path, oversized_editorconfig_body()).expect("big");
        let len = std::fs::metadata(&big_path).expect("meta").len();
        assert!(len > MAX_EDITORCONFIG_BYTES);
        let mut big = std::fs::File::open(&big_path).expect("open big");
        let got = read_capped(&mut big).expect("read big");
        assert_eq!(got, None);
    }

    #[test]
    #[cfg(unix)]
    fn a_fifo_editorconfig_returns_promptly() {
        use std::os::unix::fs::FileTypeExt;

        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(
            parent.join(".editorconfig"),
            "root = true\n[*]\nindent_size = 8\n",
        )
        .expect("parent config");
        let fifo = child.join(".editorconfig");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        assert!(made.success());
        let meta = std::fs::metadata(&fifo).expect("fifo meta");
        assert!(meta.file_type().is_fifo());
        assert!(!meta.is_file());
        let start = child.join("lib.rs");
        let (send, recv) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let over = read_editorconfig(&start);
            let _ = send.send(over);
        });
        let over = recv
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("read_editorconfig returns");
        worker.join().expect("worker");
        assert_eq!(over.indent_width, Some(8));
        assert_eq!(over.tab_width, None);
    }
}
