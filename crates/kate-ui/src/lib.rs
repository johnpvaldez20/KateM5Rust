// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! GPUI shell for one read-only document.

mod cache;
mod fixture;
mod highlight;
mod layout;
mod profile;

#[cfg(target_os = "macos")]
mod paint;
#[cfg(target_os = "macos")]
mod session;

#[cfg(target_os = "macos")]
gpui::actions!(kate_ui, [Quit]);

#[cfg(target_os = "macos")]
pub(crate) use gpui::ShapedLine;

#[cfg(not(target_os = "macos"))]
#[derive(Clone)]
pub(crate) struct ShapedLine;

use std::collections::HashMap;
use std::path::PathBuf;

/// The one GPUI entity that owns the editor model. Everything on screen is a function of this.
pub struct App {
    pub workspace: kate_core::Workspace,
}

/// GPUI element that paints one View of one Document.
///
/// The shape cache lives on the window root. This value does not own it.
#[cfg(target_os = "macos")]
pub struct EditorElement {
    /// Kept so a later packet can paint this view without a second lookup.
    #[allow(dead_code)]
    view: kate_core::ViewId,
    line_height: gpui::Pixels,
}

/// Shaped text per visible logical line, keyed by (line index, buffer revision, theme revision).
#[derive(Default)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct ShapedLineCache {
    lines: HashMap<(u32, kate_text::Revision), ShapedLine>,
}

/// Frame-time telemetry written to stderr as JSON lines when `KATEM5_PROFILE` is `1`.
///
/// `paint_micros` is the wall time of one paint.
/// `highlight_micros`, `shape_micros`, and `draw_micros` are parts of that paint.
#[derive(serde::Serialize)]
pub struct FrameSample {
    pub frame: u64,
    pub paint_micros: u64,
    pub first_line: u32,
    pub lines_painted: u32,
    pub highlight_micros: u64,
    pub shape_micros: u64,
    pub draw_micros: u64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl ShapedLineCache {
    pub(crate) fn insert(&mut self, line: u32, revision: kate_text::Revision, shaped: ShapedLine) {
        self.lines.insert((line, revision), shaped);
    }

    pub(crate) fn get(&self, line: u32, revision: kate_text::Revision) -> Option<&ShapedLine> {
        self.lines.get(&(line, revision))
    }

    /// Drop lines far from the viewport and lines from an older revision.
    #[cfg(target_os = "macos")]
    pub(crate) fn retain_near(&mut self, first: u32, last: u32, revision: kate_text::Revision) {
        let start = first.saturating_sub(400);
        let end = last.saturating_add(400);
        self.lines
            .retain(|&(line, rev), _| rev == revision && line >= start && line < end);
    }

    /// Drop shaped lines in `start..end` for one revision.
    ///
    /// A line that was drawn plain is removed after syntax colors become known.
    pub(crate) fn invalidate_range(&mut self, start: u32, end: u32, revision: kate_text::Revision) {
        if start >= end {
            return;
        }
        self.lines
            .retain(|&(line, rev), _| rev != revision || line < start || line >= end);
    }
}

#[cfg(target_os = "macos")]
impl EditorElement {
    pub(crate) fn new(view: kate_core::ViewId, line_height: gpui::Pixels) -> Self {
        Self { view, line_height }
    }

    pub(crate) fn line_height(&self) -> gpui::Pixels {
        self.line_height
    }
}

/// Open one window and block until quit.
///
/// On a host that is not macOS, print an error and exit.
pub fn run(path: Option<PathBuf>) -> ! {
    #[cfg(target_os = "macos")]
    {
        session::launch(path);
        std::process::exit(0);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        eprintln!("kate-ui requires macOS.");
        std::process::exit(1);
    }
}

pub use fixture::write_rust_fixture;
