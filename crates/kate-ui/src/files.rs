// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Open, save, and close for one workspace.
//!
//! This module does not import GPUI.
//! `session` sends a [`FileRequest`] and reads the [`FileEffect`].

use std::path::{Path, PathBuf};

/// One file action from the window.
#[derive(Debug)]
pub enum FileRequest {
    /// Make a document and a view.
    New,
    /// Open `path` and make a view.
    OpenPath(PathBuf),
    /// Write the document to its path.
    Save,
    /// Write the document to `path`.
    SaveAsPath(PathBuf),
    /// Close the view.
    Close,
}

/// What [`apply_file_request`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum FileEffect {
    /// The request finished.
    Ready,
    /// Save needs a path.
    NeedsPath,
    /// The only view of a modified document stayed open.
    RefusedModified(kate_core::DocumentId),
    /// The path did not open. No document was added.
    OpenFailed,
    /// The write failed.
    SaveFailed,
    /// Save as names a path that another document already has.
    AlreadyOpen,
    /// The view is not in the workspace.
    UnknownView,
}

/// Apply one file request to `view`.
pub fn apply_file_request(
    ws: &mut kate_core::Workspace,
    view: kate_core::ViewId,
    request: FileRequest,
) -> FileEffect {
    match request {
        FileRequest::New => make_new(ws),
        FileRequest::OpenPath(path) => open_path(ws, &path),
        FileRequest::Save => write_save(ws, view),
        FileRequest::SaveAsPath(path) => write_save_as(ws, view, path),
        FileRequest::Close => close_one(ws, view),
    }
}

/// Map a new view to a file effect.
///
/// `None` means the view is missing. The caller keeps the document.
fn effect_for_new_view(view: Option<kate_core::ViewId>) -> FileEffect {
    match view {
        None => FileEffect::OpenFailed,
        Some(_) => FileEffect::Ready,
    }
}

fn make_new(ws: &mut kate_core::Workspace) -> FileEffect {
    let doc = ws.new_document();
    effect_for_new_view(ws.new_view(doc))
}

fn open_path(ws: &mut kate_core::Workspace, path: &Path) -> FileEffect {
    let opened = match ws.open(path) {
        Ok(doc) => doc,
        Err(_) => return FileEffect::OpenFailed,
    };
    if ws.new_view(opened).is_none() {
        FileEffect::OpenFailed
    } else {
        FileEffect::Ready
    }
}

fn write_save(ws: &mut kate_core::Workspace, view: kate_core::ViewId) -> FileEffect {
    map_command(ws.execute(view, kate_core::Command::Save))
}

fn write_save_as(
    ws: &mut kate_core::Workspace,
    view: kate_core::ViewId,
    path: PathBuf,
) -> FileEffect {
    map_command(ws.execute(view, kate_core::Command::SaveAs(path)))
}

fn map_command(result: Result<(), kate_core::CommandError>) -> FileEffect {
    match result {
        Ok(()) => FileEffect::Ready,
        Err(kate_core::CommandError::NoFile) => FileEffect::NeedsPath,
        Err(kate_core::CommandError::AlreadyOpen) => FileEffect::AlreadyOpen,
        Err(kate_core::CommandError::UnknownView(_)) => FileEffect::UnknownView,
        Err(
            kate_core::CommandError::Save(_)
            | kate_core::CommandError::Load(_)
            | kate_core::CommandError::Apply(_)
            | kate_core::CommandError::Transaction(_),
        ) => FileEffect::SaveFailed,
    }
}

fn close_one(ws: &mut kate_core::Workspace, view: kate_core::ViewId) -> FileEffect {
    if ws.views.get(view).is_none() {
        return FileEffect::UnknownView;
    }
    match ws.close_view(view) {
        kate_core::CloseResult::Closed => {
            if ws.views.is_empty() {
                let doc = ws.new_document();
                ws.new_view(doc);
            }
            FileEffect::Ready
        }
        kate_core::CloseResult::RefusedModified(doc) => FileEffect::RefusedModified(doc),
    }
}

/// Return one sentence for an open or save failure.
///
/// Ready has no sentence.
#[cfg_attr(not(test), allow(dead_code))]
pub fn file_effect_line(effect: &FileEffect) -> Option<&'static str> {
    match effect {
        FileEffect::OpenFailed => Some("The file could not be opened."),
        FileEffect::SaveFailed => Some("The save failed."),
        FileEffect::NeedsPath => Some("This document has no path."),
        FileEffect::AlreadyOpen => Some("Another document has that path."),
        FileEffect::UnknownView => Some("The view is not open."),
        FileEffect::Ready | FileEffect::RefusedModified(_) => None,
    }
}

/// Return the reload prompt sentence for `choice`.
///
/// Quiet has no sentence.
#[cfg_attr(not(test), allow(dead_code))]
pub fn reload_choice_line(choice: kate_core::ReloadChoice) -> Option<&'static str> {
    choice.sentence()
}

/// File-effect tests need `kate_syntax` to build a workspace.
/// `kate-ui` links that crate only on macOS.
/// `rust (macos-15)` runs these tests.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{FileEffect, FileRequest, apply_file_request};
    use kate_core::Command;
    use std::path::{Path, PathBuf};

    fn workspace() -> kate_core::Workspace {
        let theme = kate_syntax::Theme::from_json(
            r#"{"metadata":{"name":"T","revision":1,"license":"MIT"},"text-styles":{},"editor-colors":{}}"#,
        )
        .expect("theme");
        kate_core::Workspace::new(
            kate_syntax::Repository::empty(),
            theme,
            kate_core::EditorConfig::default(),
        )
    }

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("katem5-pkt0051-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn buffer_bytes(ws: &kate_core::Workspace, doc: kate_core::DocumentId) -> Vec<u8> {
        ws.documents
            .get(doc)
            .expect("document")
            .buffer
            .text()
            .to_string()
            .into_bytes()
    }

    #[test]
    fn save_as_writes_the_bytes() {
        let dir = TempTree::new("save");
        let path = dir.path().join("note.txt");
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc).expect("view");
        ws.execute(view, Command::InsertText("hello\n".into()))
            .expect("type");
        let effect = apply_file_request(&mut ws, view, FileRequest::SaveAsPath(path.clone()));
        assert_eq!(effect, FileEffect::Ready);
        assert_eq!(
            std::fs::read(&path).unwrap_or_default(),
            buffer_bytes(&ws, doc)
        );
        let effect = apply_file_request(&mut ws, view, FileRequest::Save);
        assert_eq!(effect, FileEffect::Ready);
        assert_eq!(std::fs::read(&path).unwrap_or_default(), b"hello\n");
    }

    #[test]
    fn save_of_untitled_needs_a_path() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc).expect("view");
        ws.execute(view, Command::InsertText("draft".into()))
            .expect("type");
        let effect = apply_file_request(&mut ws, view, FileRequest::Save);
        assert_eq!(effect, FileEffect::NeedsPath);
        let document = ws.documents.get(doc).expect("document");
        assert!(document.file.is_none());
        assert!(document.history.is_modified());
        assert!(ws.views.contains_key(view));
    }

    #[test]
    fn close_refuses_a_modified_document() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc).expect("view");
        ws.execute(view, Command::InsertText("x".into()))
            .expect("type");
        let effect = apply_file_request(&mut ws, view, FileRequest::Close);
        assert_eq!(effect, FileEffect::RefusedModified(doc));
        assert!(ws.views.contains_key(view));
        assert!(ws.documents.contains_key(doc));

        let clean = ws.new_document();
        let clean_view = ws.new_view(clean).expect("view");
        let effect = apply_file_request(&mut ws, clean_view, FileRequest::Close);
        assert_eq!(effect, FileEffect::Ready);
        assert!(!ws.views.contains_key(clean_view));
    }

    #[test]
    fn missing_path_does_not_open_a_document() {
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc).expect("view");
        let before = ws.documents.len();
        let missing = std::env::temp_dir().join(format!(
            "katem5-pkt0051-missing-{}-no-such.txt",
            std::process::id()
        ));
        let effect = apply_file_request(&mut ws, view, FileRequest::OpenPath(missing));
        assert_eq!(effect, FileEffect::OpenFailed);
        assert_eq!(ws.documents.len(), before);
    }

    #[test]
    fn close_of_the_last_view_leaves_a_live_view() {
        // One view: closing it leaves a fresh live view.
        let mut ws = workspace();
        let doc = ws.new_document();
        let view = ws.new_view(doc).expect("view");
        let effect = apply_file_request(&mut ws, view, FileRequest::Close);
        assert_eq!(effect, FileEffect::Ready);
        assert!(!ws.views.contains_key(view));
        assert!(!ws.documents.contains_key(doc));
        let new_view = ws.active_view.expect("active view after last close");
        assert!(ws.views.contains_key(new_view));
        assert_ne!(new_view, view);
        assert_eq!(ws.views.len(), 1);
        assert_eq!(ws.documents.len(), 1);

        // Two views: closing one leaves the other, no blank added.
        let mut ws = workspace();
        let d1 = ws.new_document();
        let d2 = ws.new_document();
        let v1 = ws.new_view(d1).expect("view 1");
        let v2 = ws.new_view(d2).expect("view 2");
        let effect = apply_file_request(&mut ws, v1, FileRequest::Close);
        assert_eq!(effect, FileEffect::Ready);
        assert_eq!(ws.views.len(), 1);
        assert!(ws.views.contains_key(v2));
        assert_eq!(ws.documents.len(), 1);
        assert!(ws.documents.contains_key(d2));

        // Unknown view: no document added.
        let mut ws = workspace();
        let effect = apply_file_request(&mut ws, kate_core::ViewId::default(), FileRequest::Close);
        assert_eq!(effect, FileEffect::UnknownView);
        assert_eq!(ws.documents.len(), 0);
    }
}

/// The helper has no workspace. Linux CI can build this module.
#[cfg(test)]
mod effect_tests {
    use super::{FileEffect, effect_for_new_view};

    #[test]
    fn a_missing_view_is_open_failed() {
        assert_eq!(effect_for_new_view(None), FileEffect::OpenFailed);
        assert_eq!(
            effect_for_new_view(Some(kate_core::ViewId::default())),
            FileEffect::Ready
        );
    }
}

/// The helper has no workspace. Linux CI can build this module.
#[cfg(test)]
mod prompt_tests {
    use super::{FileEffect, file_effect_line, reload_choice_line};
    use kate_core::{DocumentId, ReloadChoice};

    #[test]
    fn reload_choice_line_names_the_prompt() {
        assert_eq!(reload_choice_line(ReloadChoice::Quiet), None);
        assert_eq!(
            reload_choice_line(ReloadChoice::Reload),
            Some("Reload the file from disk.")
        );
        assert_eq!(
            reload_choice_line(ReloadChoice::Ask),
            Some("The file on disk changed.")
        );
    }

    #[test]
    fn file_effect_line_names_a_failure() {
        assert_eq!(
            file_effect_line(&FileEffect::OpenFailed),
            Some("The file could not be opened.")
        );
        assert_eq!(
            file_effect_line(&FileEffect::SaveFailed),
            Some("The save failed.")
        );
        assert_eq!(
            file_effect_line(&FileEffect::NeedsPath),
            Some("This document has no path.")
        );
        assert_eq!(
            file_effect_line(&FileEffect::AlreadyOpen),
            Some("Another document has that path.")
        );
        assert_eq!(
            file_effect_line(&FileEffect::UnknownView),
            Some("The view is not open.")
        );
        assert_eq!(file_effect_line(&FileEffect::Ready), None);
        assert_eq!(
            file_effect_line(&FileEffect::RefusedModified(DocumentId::default())),
            None
        );
    }
}
