// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! One open document.
//!
//! The document owns the text, the undo history, the file state, and the syntax cache.
//! A view does not own those values. It points at a [`DocumentId`].

use std::path::PathBuf;
use std::time::SystemTime;

use kate_fileio::{Encoding, LineEnding};
use kate_text::{Anchor, History, TextBuffer};

use crate::syntax_cache::SyntaxCache;

#[path = "modeline.rs"]
mod modeline;

slotmap::new_key_type! {
    /// Key for one document in a [`crate::Workspace`].
    pub struct DocumentId;
}

/// One open buffer and the state that belongs to it.
pub struct Document {
    /// The rope and the edit log.
    pub buffer: TextBuffer,
    /// Undo and redo for `buffer`.
    pub history: History,
    /// `None` when the document has no path.
    pub file: Option<FileState>,
    /// Per-line highlight state.
    pub syntax: SyntaxCache,
    /// Marks anchored at line starts.
    pub marks: Vec<Mark>,
    /// Name shown for the document.
    pub name: String,
    /// Indent and save policy for this document.
    pub config: DocumentConfig,
}

/// Bytes on disk for a document that has a path.
#[derive(Clone, Debug)]
pub struct FileState {
    /// Canonical path when the platform can resolve one.
    pub path: PathBuf,
    /// Encoding used to read and write the file.
    pub encoding: Encoding,
    /// `true` when the file starts with a byte-order mark.
    pub bom: bool,
    /// Line ending written on save.
    pub line_ending: LineEnding,
    /// SHA-256 of the bytes last read or written.
    pub digest_on_disk: [u8; 32],
    /// Modified time last read or written.
    pub mtime_on_disk: Option<SystemTime>,
    /// Set when a later packet sees a disk change.
    pub disk_change: Option<DiskChange>,
}

/// How the file on disk differs from the open buffer.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiskChange {
    /// The bytes on disk changed.
    Modified,
    /// The path is gone.
    Deleted,
    /// The path exists again.
    Created,
}

/// The reload prompt for one disk change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadChoice {
    /// The file on disk matches the last read.
    Quiet,
    /// The buffer is clean. Reload the file.
    Reload,
    /// Ask before a reload.
    Ask,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ReloadChoice {
    /// Return one sentence for this reload choice.
    ///
    /// Quiet has no sentence.
    pub fn sentence(self) -> Option<&'static str> {
        match self {
            Self::Quiet => None,
            Self::Reload => Some("Reload the file from disk."),
            Self::Ask => Some("The file on disk changed."),
        }
    }
}

impl FileState {
    /// Set `disk_change` from the file that is now on disk.
    pub fn note_disk(&mut self, exists: bool, digest: [u8; 32]) {
        self.disk_change = match exists {
            false => Some(DiskChange::Deleted),
            true if self.disk_change == Some(DiskChange::Deleted) => Some(DiskChange::Created),
            true if digest != self.digest_on_disk => Some(DiskChange::Modified),
            true => None,
        };
    }

    /// Choose a reload prompt from `disk_change` and `dirty`.
    pub fn reload_choice(&self, dirty: bool) -> ReloadChoice {
        match (self.disk_change, dirty) {
            (None, _) => ReloadChoice::Quiet,
            (Some(DiskChange::Deleted), _) => ReloadChoice::Ask,
            (Some(DiskChange::Modified | DiskChange::Created), false) => ReloadChoice::Reload,
            (Some(DiskChange::Modified | DiskChange::Created), true) => ReloadChoice::Ask,
        }
    }
}

/// A mark anchored at a line start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mark {
    /// Position that moves with the text.
    pub anchor: Anchor,
    /// Kind of mark.
    pub kind: MarkKind,
}

/// The kind of a mark.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MarkKind {
    /// A bookmark.
    Bookmark,
    /// An active breakpoint.
    BreakpointActive,
    /// A disabled breakpoint.
    BreakpointDisabled,
    /// The current execution line.
    Execution,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// Indent and trailing-space policy for one document.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentConfig {
    /// Width of one tab stop. `0` is treated as `1` by movement.
    pub tab_width: u8,
    /// Columns added by an indent step.
    pub indent_width: u8,
    /// `true` when an indent step uses spaces.
    pub indent_with_spaces: bool,
    /// Which lines lose trailing spaces on save.
    pub remove_trailing_spaces: TrailingSpaces,
    /// `true` when save adds a final line ending if the buffer has none.
    pub ensure_trailing_newline: bool,
}

impl Default for DocumentConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            indent_width: 4,
            indent_with_spaces: true,
            remove_trailing_spaces: TrailingSpaces::Never,
            ensure_trailing_newline: false,
        }
    }
}

/// Which lines lose trailing spaces on save.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TrailingSpaces {
    /// Do not remove trailing spaces.
    Never,
    /// Remove trailing spaces on lines edited since the last save.
    ModifiedLines,
    /// Remove trailing spaces on every line.
    All,
}

impl Document {
    pub(crate) fn blank(name: String, config: DocumentConfig) -> Self {
        let buffer = TextBuffer::new();
        let mut syntax = SyntaxCache::new(None);
        syntax.fit(buffer.line_count());
        Self {
            buffer,
            history: History::new(),
            file: None,
            syntax,
            marks: Vec::new(),
            name,
            config,
        }
    }

    pub(crate) fn from_loaded(
        name: String,
        config: DocumentConfig,
        text: &str,
        file: FileState,
        definition: Option<kate_syntax::DefId>,
    ) -> Self {
        let mut config = config;
        let mut file = file;
        // `.editorconfig` applies first. A modeline wins on each field it sets.
        let editor = modeline::read_editorconfig(&file.path);
        modeline::apply_mode(&mut config, &mut file, &editor);
        let mode = modeline::modeline_overrides(text);
        modeline::apply_mode(&mut config, &mut file, &mode);
        let buffer = TextBuffer::from_str(text);
        let mut syntax = SyntaxCache::new(definition);
        syntax.fit(buffer.line_count());
        Self {
            buffer,
            history: History::new(),
            file: Some(file),
            syntax,
            marks: Vec::new(),
            name,
            config,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use kate_fileio::{DecodeOptions, LineEnding};

    use super::{DiskChange, Document, DocumentConfig, FileState, ReloadChoice, TrailingSpaces};

    fn file_state(path: PathBuf, ending: LineEnding) -> FileState {
        FileState {
            path,
            encoding: DecodeOptions::default().fallback,
            bom: true,
            line_ending: ending,
            digest_on_disk: [9; 32],
            mtime_on_disk: Some(SystemTime::UNIX_EPOCH),
            disk_change: Some(DiskChange::Modified),
        }
    }

    #[test]
    fn plain_file_keeps_the_given_config() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("parent");
        let dir = parent.join("dir");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let hostile = "\
[*]
indent_size = 8
tab_width = 6
indent_style = space
trim_trailing_whitespace = false
insert_final_newline = false
end_of_line = crlf
charset = utf-8
";
        std::fs::write(parent.join(".editorconfig"), hostile).expect("parent config");
        std::fs::write(dir.join(".editorconfig"), "root = true\n").expect("root config");
        let path = dir.join("plain.txt");
        let text = "hello\n";
        std::fs::write(&path, text).expect("file");
        let config = DocumentConfig {
            tab_width: 7,
            indent_width: 3,
            indent_with_spaces: false,
            remove_trailing_spaces: TrailingSpaces::ModifiedLines,
            ensure_trailing_newline: true,
        };
        let file = file_state(path, LineEnding::Lf);
        let encoding = file.encoding;
        let doc = Document::from_loaded("plain.txt".to_string(), config.clone(), text, file, None);
        assert_eq!(doc.config, config);
        let saved = doc.file.expect("file");
        assert_eq!(saved.line_ending, LineEnding::Lf);
        assert_eq!(saved.encoding, encoding);
        assert!(saved.bom);
        assert_eq!(saved.digest_on_disk, [9; 32]);
        assert_eq!(saved.mtime_on_disk, Some(SystemTime::UNIX_EPOCH));
        assert_eq!(saved.disk_change, Some(DiskChange::Modified));
        assert_eq!(doc.buffer.text().to_string(), text);
    }

    #[test]
    fn blank_keeps_the_given_config() {
        let config = DocumentConfig {
            tab_width: 7,
            indent_width: 3,
            indent_with_spaces: false,
            remove_trailing_spaces: TrailingSpaces::All,
            ensure_trailing_newline: true,
        };
        let doc = Document::blank("scratch".to_string(), config.clone());
        assert_eq!(doc.config, config);
        assert!(doc.file.is_none());
    }

    #[test]
    fn from_loaded_applies_editorconfig_then_the_modeline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_text = "\
root = true
[*]
indent_size = 8
tab_width = 6
indent_style = tab
insert_final_newline = true
end_of_line = lf
";
        std::fs::write(dir.path().join(".editorconfig"), config_text).expect("config");
        let path = dir.path().join("main.rs");
        let text = "// kate: indent-width 2; replace-tabs on; eol dos;\nfn main() {}\n";
        std::fs::write(&path, text).expect("file");
        let given = DocumentConfig {
            indent_with_spaces: false,
            ..DocumentConfig::default()
        };
        let file = file_state(path, LineEnding::Cr);
        let doc = Document::from_loaded("main.rs".to_string(), given, text, file, None);
        assert_eq!(doc.config.tab_width, 6);
        assert!(doc.config.ensure_trailing_newline);
        assert_eq!(doc.config.indent_width, 2);
        assert!(doc.config.indent_with_spaces);
        let saved = doc.file.expect("file");
        assert_eq!(saved.line_ending, LineEnding::CrLf);
        assert!(saved.bom);
        assert_eq!(doc.buffer.text().to_string(), text);
    }

    #[test]
    fn note_disk_names_modified_deleted_and_created() {
        let mut file = file_state(PathBuf::from("plain.txt"), LineEnding::Lf);
        let stored = file.digest_on_disk;
        file.note_disk(true, stored);
        assert_eq!(file.disk_change, None);
        assert_eq!(file.digest_on_disk, stored);

        let mut other = stored;
        other[0] = other[0].wrapping_add(1);
        file.note_disk(true, other);
        assert_eq!(file.disk_change, Some(DiskChange::Modified));
        assert_eq!(file.digest_on_disk, stored);

        file.note_disk(false, other);
        assert_eq!(file.disk_change, Some(DiskChange::Deleted));
        assert_eq!(file.digest_on_disk, stored);

        file.note_disk(true, stored);
        assert_eq!(file.disk_change, Some(DiskChange::Created));
        assert_eq!(file.digest_on_disk, stored);

        file.note_disk(true, stored);
        assert_eq!(file.disk_change, None);
        assert_eq!(file.digest_on_disk, stored);
    }

    #[test]
    fn reload_choice_asks_when_the_buffer_is_dirty() {
        let mut file = file_state(PathBuf::from("plain.txt"), LineEnding::Lf);

        file.disk_change = None;
        assert_eq!(file.reload_choice(false), ReloadChoice::Quiet);
        assert_eq!(file.reload_choice(true), ReloadChoice::Quiet);

        file.disk_change = Some(DiskChange::Modified);
        assert_eq!(file.reload_choice(false), ReloadChoice::Reload);
        assert_eq!(file.reload_choice(true), ReloadChoice::Ask);

        file.disk_change = Some(DiskChange::Created);
        assert_eq!(file.reload_choice(false), ReloadChoice::Reload);
        assert_eq!(file.reload_choice(true), ReloadChoice::Ask);

        file.disk_change = Some(DiskChange::Deleted);
        assert_eq!(file.reload_choice(false), ReloadChoice::Ask);
        assert_eq!(file.reload_choice(true), ReloadChoice::Ask);
    }

    #[test]
    fn reload_choice_sentence_names_the_prompt() {
        assert_eq!(ReloadChoice::Quiet.sentence(), None);
        assert_eq!(
            ReloadChoice::Reload.sentence(),
            Some("Reload the file from disk.")
        );
        assert_eq!(
            ReloadChoice::Ask.sentence(),
            Some("The file on disk changed.")
        );
    }

    #[test]
    fn reload_prompt_line_names_a_dirty_change() {
        assert_eq!(super::reload_prompt_line(None, false), None);

        let mut file = file_state(PathBuf::from("plain.txt"), LineEnding::Lf);
        assert_eq!(
            super::reload_prompt_line(Some(&file), false),
            Some("Reload the file from disk.")
        );
        assert_eq!(
            super::reload_prompt_line(Some(&file), true),
            Some("The file on disk changed.")
        );

        file.disk_change = None;
        assert_eq!(super::reload_prompt_line(Some(&file), true), None);
    }
}
