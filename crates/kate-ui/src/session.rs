// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(target_os = "macos")]

//! The window, the scroll position, and the IME composition.
//!
//! The document stays read-only. Marked text is drawn on top of the caret.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::OnceLock;

use gpui::{
    App as GpuiApp, Application, Bounds, Context, EntityInputHandler, FocusHandle, Focusable,
    KeyBinding, Menu, MenuItem, Pixels, Point, Render, ScrollWheelEvent, TitlebarOptions,
    UTF16Selection, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use kate_core::{Command, DocumentId, EditorConfig, OpenError, Scroll, ViewId, Workspace};
use kate_syntax::{DefId, Highlighter, LineHighlight, Repository, Theme};
use kate_text::Revision;

use crate::cache::KeyedCache;
use crate::highlight::{CATCH_UP_LINE_BUDGET, PAINT_LINE_BUDGET, highlight_step};
use crate::paint::{
    self, Colors, DrawnFrame, EditorSurface, byte_at_char, line_text, runs_for_line,
};
use crate::{App, EditorElement, ShapedLine, ShapedLineCache, profile};

const LINE_HEIGHT: f32 = 20.0;
const SAMPLE: &str = "\
fn main() {\n\
    let greeting = \"hello\";\n\
    println!(\"{greeting}\");\n\
}\n\
// 日本語 😀\n";

pub(crate) struct ViewSnap {
    pub scroll: Scroll,
    pub scroll_y: f32,
    pub scroll_x: f32,
    pub revision: Revision,
    pub line_count: u32,
    pub caret_line: u32,
    pub caret_byte: usize,
    pub colors: Colors,
    pub font: gpui::Font,
    pub column_width: Pixels,
    pub column_ready: bool,
    pub ime_text: String,
    pub marked: Option<Range<usize>>,
    pub focus: FocusHandle,
    pub frame: u64,
    pub line_height: Pixels,
}

pub(crate) struct WindowRoot {
    model: gpui::Entity<App>,
    editor: EditorElement,
    focus: FocusHandle,
    doc: DocumentId,
    view: ViewId,
    hl: Option<Highlighter<'static>>,
    colors: Colors,
    font: gpui::Font,
    scroll_y: f32,
    scroll_x: f32,
    column_width: Pixels,
    column_ready: bool,
    frame: u64,
    ime_text: String,
    selected_utf8: Range<usize>,
    marked_utf8: Option<Range<usize>>,
    caret_bounds: Option<Bounds<Pixels>>,
    pub(crate) shaped: ShapedLineCache,
    pub(crate) gutter: KeyedCache<ShapedLine>,
    /// First line that `ensure` has not covered yet.
    checkpoint: u32,
    checkpoint_revision: Revision,
    catch_up_to: u32,
    catch_up_queued: bool,
    visible_first: u32,
    visible_last: u32,
}

pub(crate) fn launch(path: Option<PathBuf>) {
    Application::new().run(move |cx| {
        cx.on_action(|_: &crate::Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", crate::Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "KateM5Rust".into(),
            items: vec![MenuItem::action("Quit", crate::Quit)],
        }]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let window_bounds = Bounds::centered(None, size(px(1100.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("KateM5Rust".into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                focus: true,
                ..Default::default()
            },
            |window, cx| {
                cx.activate(true);
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window);
                    WindowRoot::open(cx, path, focus)
                })
            },
        )
        .expect("open window");
    });
}

impl WindowRoot {
    fn open(cx: &mut Context<Self>, path: Option<PathBuf>, focus: FocusHandle) -> Self {
        let mut workspace = themed_workspace();
        let (doc, view) = open_document(&mut workspace, path);
        let definition = workspace
            .documents
            .get(doc)
            .and_then(|document| document.syntax.definition);
        let hl = highlighter_for(&workspace, definition);
        let colors = paint::colors_from_theme(&workspace.theme);
        let font = paint::editor_font();
        let model = cx.new(|_| App { workspace });
        Self {
            model,
            editor: EditorElement::new(view, px(LINE_HEIGHT)),
            focus,
            doc,
            view,
            hl,
            colors,
            font,
            scroll_y: 0.0,
            scroll_x: 0.0,
            column_width: px(8.0),
            column_ready: false,
            frame: 0,
            ime_text: String::new(),
            selected_utf8: 0..0,
            marked_utf8: None,
            caret_bounds: None,
            shaped: ShapedLineCache::default(),
            gutter: KeyedCache::default(),
            checkpoint: 0,
            checkpoint_revision: Revision(0),
            catch_up_to: 0,
            catch_up_queued: false,
            visible_first: 0,
            visible_last: 0,
        }
    }

    pub(crate) fn snapshot(&self, app: &GpuiApp) -> ViewSnap {
        let model = self.model.read(app);
        let (revision, line_count, caret_line, caret_byte, scroll) =
            document_view(model, self.doc, self.view);
        ViewSnap {
            scroll,
            scroll_y: self.scroll_y,
            scroll_x: self.scroll_x,
            revision,
            line_count,
            caret_line,
            caret_byte,
            colors: self.colors,
            font: self.font.clone(),
            column_width: self.column_width,
            column_ready: self.column_ready,
            ime_text: self.ime_text.clone(),
            marked: self.marked_utf8.clone(),
            focus: self.focus.clone(),
            frame: self.frame,
            line_height: self.editor.line_height(),
        }
    }

    /// Highlight only a short span from the checkpoint.
    ///
    /// A viewport past that span is left plain. Catch-up runs after paint.
    pub(crate) fn prepare_highlight(
        &mut self,
        first: u32,
        last: u32,
        cx: &mut Context<Self>,
    ) -> HighlightPrep {
        let revision = self.document_revision(cx);
        if self.checkpoint_revision != revision {
            self.checkpoint = 0;
            self.checkpoint_revision = revision;
            self.catch_up_to = 0;
        }
        self.visible_first = first;
        self.visible_last = last.saturating_add(1);
        if self.hl.is_none() {
            self.checkpoint = last.saturating_add(1);
            return HighlightPrep {
                drop_from: 0,
                drop_to: 0,
                plain_from: None,
                want_catch_up: false,
            };
        }
        let step = highlight_step(self.checkpoint, first, last, PAINT_LINE_BUDGET);
        let drop_from = self.checkpoint;
        if let Some(through) = step.ensure_through {
            self.ensure_syntax(through, cx);
            self.checkpoint = through.saturating_add(1);
        }
        let want_catch_up = self.checkpoint <= last;
        if want_catch_up {
            self.catch_up_to = last;
        }
        HighlightPrep {
            drop_from,
            drop_to: self.checkpoint,
            plain_from: step.plain_from,
            want_catch_up,
        }
    }

    pub(crate) fn plans_for(
        &mut self,
        lines: &[u32],
        plain_from: Option<u32>,
        cx: &mut Context<Self>,
    ) -> Vec<paint::LinePlan> {
        let mut plans = Vec::with_capacity(lines.len());
        let model = self.model.clone();
        let doc = self.doc;
        let font = self.font.clone();
        let foreground = self.colors.foreground;
        if self.hl.is_none() {
            model.update(cx, |app, _| {
                push_plain(&mut plans, &app.workspace, doc, lines, &font, foreground);
            });
            return plans;
        }
        let syntax: Vec<u32> = lines
            .iter()
            .copied()
            .filter(|line| plain_from.is_none_or(|start| *line < start))
            .collect();
        let plain: Vec<u32> = lines
            .iter()
            .copied()
            .filter(|line| plain_from.is_some_and(|start| *line >= start))
            .collect();
        if let Some(hl) = self.hl.as_mut() {
            model.update(cx, |app, _| {
                push_plans(
                    &mut plans,
                    &app.workspace,
                    doc,
                    &syntax,
                    hl,
                    &font,
                    foreground,
                );
                push_plain(&mut plans, &app.workspace, doc, &plain, &font, foreground);
            });
        }
        plans
    }

    pub(crate) fn apply_frame(
        &mut self,
        drawn: DrawnFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.column_width = drawn.column_width;
        self.column_ready = true;
        self.caret_bounds = drawn.caret_bounds;
        self.frame = drawn.sample.frame;
        if drawn.want_catch_up && !self.catch_up_queued {
            self.catch_up_queued = true;
            cx.defer_in(window, |this, window, cx| {
                this.run_catch_up(window, cx);
            });
        }
        profile::write_sample(&drawn.sample);
    }

    fn run_catch_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hl.is_none() || self.checkpoint > self.catch_up_to {
            self.catch_up_queued = false;
            return;
        }
        let revision = self.document_revision(cx);
        if self.checkpoint_revision != revision {
            self.checkpoint = 0;
            self.checkpoint_revision = revision;
        }
        if self.checkpoint > self.catch_up_to {
            self.catch_up_queued = false;
            return;
        }
        let from = self.checkpoint;
        let through = from
            .saturating_add(CATCH_UP_LINE_BUDGET)
            .saturating_sub(1)
            .min(self.catch_up_to);
        self.ensure_syntax(through, cx);
        self.checkpoint = through.saturating_add(1);
        self.shaped
            .invalidate_range(from, self.checkpoint, revision);
        let overlaps = from < self.visible_last && self.checkpoint > self.visible_first;
        let more = self.checkpoint <= self.catch_up_to;
        if overlaps {
            cx.notify();
        }
        if more {
            cx.defer_in(window, |this, window, cx| {
                this.run_catch_up(window, cx);
            });
        } else {
            self.catch_up_queued = false;
        }
    }

    fn ensure_syntax(&mut self, through: u32, cx: &mut Context<Self>) {
        let model = self.model.clone();
        let doc = self.doc;
        if let Some(hl) = self.hl.as_mut() {
            model.update(cx, |app, _| {
                if let Some(document) = app.workspace.documents.get_mut(doc) {
                    document.syntax.ensure(&document.buffer, hl, through);
                }
            });
        }
    }

    fn document_revision(&self, cx: &Context<Self>) -> Revision {
        self.model
            .read(cx)
            .workspace
            .documents
            .get(self.doc)
            .map(|document| document.buffer.revision())
            .unwrap_or(Revision(0))
    }

    fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = f32::from(self.editor.line_height()).max(1.0);
        let delta = event.delta.pixel_delta(self.editor.line_height());
        // A positive wheel delta moves the document toward line 0.
        self.scroll_y = (self.scroll_y - f32::from(delta.y)).max(0.0);
        self.scroll_x = (self.scroll_x - f32::from(delta.x)).clamp(0.0, 20_000.0);
        let line_count = self.model.update(cx, |app, _| {
            app.workspace
                .documents
                .get(self.doc)
                .map(|document| document.buffer.line_count())
                .unwrap_or(1)
        });
        let max_line = line_count.saturating_sub(1);
        let max_y = max_line as f32 * line_height;
        if self.scroll_y > max_y {
            self.scroll_y = max_y;
        }
        let first_line = (self.scroll_y / line_height).floor() as u32;
        let column = f32::from(self.column_width).max(1.0);
        let first_col = (self.scroll_x / column).floor() as u32;
        let view = self.view;
        self.model.update(cx, |app, _| {
            let _ = app.workspace.execute(view, Command::ScrollTo(first_line));
            if let Some(state) = app.workspace.views.get_mut(view) {
                state.scroll.first_col = first_col;
            }
        });
        cx.notify();
    }
}

impl Render for WindowRoot {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = self.colors.background;
        let focus = self.focus.clone();
        div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(background)
            .cursor(gpui::CursorStyle::IBeam)
            .track_focus(&focus)
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(EditorSurface { root: cx.entity() })
    }
}

impl Focusable for WindowRoot {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for WindowRoot {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = range_from_utf16(&self.ime_text, &range_utf16);
        *actual_range = Some(range_to_utf16(&self.ime_text, &range));
        Some(self.ime_text.get(range)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.ime_text, &self.selected_utf8),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_utf8
            .as_ref()
            .map(|range| range_to_utf16(&self.ime_text, range))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked_utf8 = None;
        self.ime_text.clear();
        self.selected_utf8 = 0..0;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        _text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The spike does not edit the document. Drop the committed composition.
        self.ime_text.clear();
        self.marked_utf8 = None;
        self.selected_utf8 = 0..0;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = new_text.replace(['\n', '\r'], "");
        let marked = self.marked_utf8.clone();
        let selected = self.selected_utf8.clone();
        let range = range_utf16
            .as_ref()
            .map(|range| range_from_utf16(&self.ime_text, range))
            .or(marked)
            .unwrap_or(selected);
        let start = range.start.min(self.ime_text.len());
        let end = range.end.min(self.ime_text.len()).max(start);
        self.ime_text.replace_range(start..end, &new_text);
        self.marked_utf8 = if new_text.is_empty() {
            None
        } else {
            Some(start..start + new_text.len())
        };
        self.selected_utf8 = if let Some(selected) = new_selected_range {
            let from = start + utf8_index(&new_text, selected.start);
            let to = start + utf8_index(&new_text, selected.end);
            let from = from.min(self.ime_text.len());
            let to = to.min(self.ime_text.len()).max(from);
            from..to
        } else {
            let at = (start + new_text.len()).min(self.ime_text.len());
            at..at
        };
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(self.caret_bounds.unwrap_or_else(|| {
            Bounds::new(
                element_bounds.origin,
                size(px(4.0), self.editor.line_height()),
            )
        }))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(utf16_index(&self.ime_text, self.selected_utf8.end))
    }
}

pub(crate) struct HighlightPrep {
    pub drop_from: u32,
    pub drop_to: u32,
    pub plain_from: Option<u32>,
    pub want_catch_up: bool,
}

fn push_plans(
    plans: &mut Vec<paint::LinePlan>,
    workspace: &Workspace,
    doc: DocumentId,
    lines: &[u32],
    hl: &mut Highlighter<'_>,
    font: &gpui::Font,
    foreground: gpui::Hsla,
) {
    let Some((revision, definition)) = workspace
        .documents
        .get(doc)
        .map(|document| (document.buffer.revision(), document.syntax.definition))
    else {
        return;
    };
    let def_name = definition.map(|id| workspace.syntax.definition(id).name.clone());
    for &line in lines {
        let (text, start) = {
            let document = workspace.documents.get(doc).expect("document");
            (
                line_text(&document.buffer, line),
                document.syntax.start_state(line),
            )
        };
        let mut highlight = LineHighlight::default();
        if def_name.is_some() {
            hl.highlight_line(&text, start, &mut highlight);
        }
        let runs = runs_for_line(
            &text,
            &highlight.spans,
            &workspace.theme,
            &workspace.syntax,
            def_name.as_deref(),
            font,
            foreground,
        );
        plans.push(paint::LinePlan {
            index: line,
            text,
            runs,
            revision,
        });
    }
}

fn push_plain(
    plans: &mut Vec<paint::LinePlan>,
    workspace: &Workspace,
    doc: DocumentId,
    lines: &[u32],
    font: &gpui::Font,
    foreground: gpui::Hsla,
) {
    let Some(revision) = workspace
        .documents
        .get(doc)
        .map(|document| document.buffer.revision())
    else {
        return;
    };
    for &line in lines {
        let text = line_text(
            &workspace.documents.get(doc).expect("document").buffer,
            line,
        );
        let runs = runs_for_line(
            &text,
            &[],
            &workspace.theme,
            &workspace.syntax,
            None,
            font,
            foreground,
        );
        plans.push(paint::LinePlan {
            index: line,
            text,
            runs,
            revision,
        });
    }
}

fn document_view(
    model: &App,
    doc: DocumentId,
    view: ViewId,
) -> (Revision, u32, u32, usize, Scroll) {
    let Some(document) = model.workspace.documents.get(doc) else {
        return (Revision(0), 1, 0, 0, Scroll::default());
    };
    let revision = document.buffer.revision();
    let line_count = document.buffer.line_count();
    let Some(state) = model.workspace.views.get(view) else {
        return (revision, line_count, 0, 0, Scroll::default());
    };
    let at = document.buffer.line_col(state.selections.primary().head);
    let text = line_text(&document.buffer, at.line);
    let byte = byte_at_char(&text, at.col);
    (revision, line_count, at.line, byte, state.scroll)
}

fn open_document(workspace: &mut Workspace, path: Option<PathBuf>) -> (DocumentId, ViewId) {
    let Some(path) = path else {
        let doc = workspace.new_document();
        let view = workspace.new_view(doc).expect("the document is open");
        let _ = workspace.execute(view, Command::InsertText(SAMPLE.into()));
        if let Some(id) = workspace.syntax.by_name("Rust") {
            let _ = workspace.execute(view, Command::SetSyntax(Some(id)));
        }
        return (doc, view);
    };
    match workspace.open(&path) {
        Ok(doc) => {
            let view = workspace.new_view(doc).expect("the document is open");
            (doc, view)
        }
        Err(err) => {
            let doc = workspace.new_document();
            let view = workspace.new_view(doc).expect("the document is open");
            let _ = workspace.execute(view, Command::InsertText(open_error(&path, &err)));
            (doc, view)
        }
    }
}

fn open_error(path: &std::path::Path, err: &OpenError) -> String {
    match err {
        OpenError::ProbablyBinary(_) => format!("The file looks binary.\n{}", path.display()),
        OpenError::Load(err) => format!("Cannot open the file.\n{err:?}\n{}", path.display()),
    }
}

fn highlighter_for(
    workspace: &Workspace,
    definition: Option<DefId>,
) -> Option<Highlighter<'static>> {
    let id = definition?;
    let name = workspace.syntax.definition(id).name.clone();
    let shared = shared_repository();
    if shared.by_name(&name) != Some(id) {
        return None;
    }
    Some(Highlighter::new(shared, id).expect("syntax rules"))
}

fn themed_workspace() -> Workspace {
    let theme = Theme::from_json(include_str!("../../kate-syntax/themes/breeze-dark.theme"))
        .expect("theme");
    Workspace::new(load_repository(), theme, EditorConfig::default())
}

fn shared_repository() -> &'static Repository {
    static REPO: OnceLock<Repository> = OnceLock::new();
    REPO.get_or_init(load_repository)
}

fn load_repository() -> Repository {
    let mut repo = Repository::empty();
    for xml in [
        include_str!("../../kate-syntax/definitions/rust.xml"),
        include_str!("../../kate-syntax/definitions/comments.xml"),
        include_str!("../../kate-syntax/definitions/alert.xml"),
        include_str!("../../kate-syntax/definitions/modelines.xml"),
        include_str!("../../kate-syntax/definitions/spdx-comments.xml"),
    ] {
        repo.add_xml(xml).expect("syntax definition");
    }
    repo.link().expect("link syntax");
    repo
}

fn utf8_index(text: &str, utf16_index: usize) -> usize {
    let mut utf8 = 0usize;
    let mut utf16 = 0usize;
    for ch in text.chars() {
        if utf16 >= utf16_index {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

fn utf16_index(text: &str, utf8_index: usize) -> usize {
    let mut utf8 = 0usize;
    let mut utf16 = 0usize;
    for ch in text.chars() {
        if utf8 >= utf8_index {
            break;
        }
        utf8 += ch.len_utf8();
        utf16 += ch.len_utf16();
    }
    utf16
}

fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    utf8_index(text, range.start)..utf8_index(text, range.end)
}

fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    utf16_index(text, range.start)..utf16_index(text, range.end)
}
