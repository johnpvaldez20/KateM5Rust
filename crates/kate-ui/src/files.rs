// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Open, save, and close for one workspace.
//!
//! This module does not import GPUI.
//! `session` sends a [`FileRequest`] and reads the [`FileEffect`].

// On macOS the module is unused until `session` calls it.
// The tests must still compile and fail on assertions.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use std::path::PathBuf;

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
    let _ = (ws, view, request);
    FileEffect::Ready
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
}
