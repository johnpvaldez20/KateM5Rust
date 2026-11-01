// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! GPUI shell for one read-only document.

mod fixture;
mod layout;
mod paint;
mod profile;
mod session;

gpui::actions!(kate_ui, [Quit]);

use std::collections::HashMap;
use std::path::PathBuf;

/// The one GPUI entity that owns the editor model. Everything on screen is a function of this.
pub struct App {
    pub workspace: kate_core::Workspace,
}

/// GPUI element that paints one View of one Document. Holds only layout caches, never model state.
pub struct EditorElement {
    view: kate_core::ViewId,
    line_height: gpui::Pixels,
    glyph_cache: ShapedLineCache,
}

/// Shaped text per visible logical line, keyed by (line index, buffer revision, theme revision).
pub struct ShapedLineCache {
    lines: HashMap<(u32, kate_text::Revision), gpui::ShapedLine>,
}

/// Frame-time telemetry written to stderr as JSON lines when KATEM5_PROFILE=1. This is the spike's deliverable.
#[derive(serde::Serialize)]
pub struct FrameSample {
    pub frame: u64,
    pub paint_micros: u64,
    pub first_line: u32,
    pub lines_painted: u32,
}

impl ShapedLineCache {
    pub(crate) fn insert(
        &mut self,
        line: u32,
        revision: kate_text::Revision,
        shaped: gpui::ShapedLine,
    ) {
        self.lines.insert((line, revision), shaped);
    }

    pub(crate) fn get(
        &self,
        line: u32,
        revision: kate_text::Revision,
    ) -> Option<&gpui::ShapedLine> {
        self.lines.get(&(line, revision))
    }

    /// Drop lines far from the viewport and lines from an older revision.
    pub(crate) fn retain_near(&mut self, first: u32, last: u32, revision: kate_text::Revision) {
        let start = first.saturating_sub(400);
        let end = last.saturating_add(400);
        self.lines
            .retain(|&(line, rev), _| rev == revision && line >= start && line < end);
    }
}

impl EditorElement {
    pub(crate) fn new(view: kate_core::ViewId, line_height: gpui::Pixels) -> Self {
        Self {
            view,
            line_height,
            glyph_cache: ShapedLineCache {
                lines: HashMap::new(),
            },
        }
    }

    pub(crate) fn view(&self) -> kate_core::ViewId {
        self.view
    }

    pub(crate) fn line_height(&self) -> gpui::Pixels {
        self.line_height
    }

    pub(crate) fn cache_mut(&mut self) -> &mut ShapedLineCache {
        &mut self.glyph_cache
    }
}

/// Open one window and block until quit.
pub fn run(path: Option<PathBuf>) -> ! {
    session::launch(path);
    std::process::exit(0)
}

pub use fixture::write_rust_fixture;
