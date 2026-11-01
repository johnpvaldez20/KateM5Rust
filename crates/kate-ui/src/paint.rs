// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paint one view. The element holds layout caches only.

use std::ops::Range;

use gpui::{
    Bounds, Element, ElementId, Entity, Font, FontFallbacks, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun,
    UnderlineStyle, Window, fill, point, px, relative, size,
};
use kate_syntax::{DefaultStyle, EditorColor, Rgba, Span, Style as ThemeStyle, Theme};
use kate_text::{Revision, TextBuffer};

use crate::session::WindowRoot;
use crate::{EditorElement, FrameSample};

const FONT_SIZE: f32 = 14.0;

#[derive(Clone, Copy)]
pub(crate) struct Colors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub gutter: Hsla,
    pub gutter_text: Hsla,
    pub current_line: Hsla,
    pub marked: Hsla,
}

pub(crate) struct LinePlan {
    pub index: u32,
    pub text: String,
    pub runs: Vec<TextRun>,
    pub revision: Revision,
}

pub(crate) struct EditorSurface {
    pub editor: EditorElement,
    pub root: Entity<WindowRoot>,
}

pub(crate) fn editor_font() -> Font {
    let mut font = gpui::font("Menlo");
    font.fallbacks = Some(FontFallbacks::from_fonts(vec![
        "Apple Color Emoji".to_string(),
        "Hiragino Sans".to_string(),
        "PingFang SC".to_string(),
        "Apple SD Gothic Neo".to_string(),
    ]));
    font
}

pub(crate) fn colors_from_theme(theme: &Theme) -> Colors {
    let normal = theme.text_style(DefaultStyle::Normal);
    let foreground = normal
        .fg
        .map(hsla_from_rgba)
        .unwrap_or_else(|| gpui::rgb(0x00D4_D4D4).into());
    let background = color_or(theme, EditorColor::BackgroundColor, gpui::rgb(0x001E_1E1E));
    Colors {
        background,
        foreground,
        gutter: color_or(theme, EditorColor::IconBorder, background),
        gutter_text: color_or(theme, EditorColor::LineNumbers, gpui::rgb(0x0085_8585)),
        current_line: color_or(theme, EditorColor::CurrentLine, background),
        marked: color_or(theme, EditorColor::TextSelection, gpui::rgba(0x3344_88AA)),
    }
}

pub(crate) fn line_text(buffer: &TextBuffer, line: u32) -> String {
    let rope = buffer.text();
    let index = line as usize;
    if index >= rope.len_lines() {
        return String::new();
    }
    let mut text = rope.line(index).to_string();
    if text.ends_with('\n') {
        text.pop();
    }
    if text.ends_with('\r') {
        text.pop();
    }
    text
}

pub(crate) fn byte_at_char(text: &str, char_index: u32) -> usize {
    text.chars()
        .take(char_index as usize)
        .map(char::len_utf8)
        .sum()
}

pub(crate) fn runs_for_line(
    text: &str,
    spans: &[Span],
    theme: &Theme,
    repo: &kate_syntax::Repository,
    def_name: Option<&str>,
    font: &Font,
    foreground: Hsla,
) -> Vec<TextRun> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut byte_of = Vec::with_capacity(text.chars().count() + 1);
    byte_of.push(0usize);
    for ch in text.chars() {
        let last = *byte_of.last().expect("byte table");
        byte_of.push(last + ch.len_utf8());
    }
    let chars = byte_of.len() - 1;
    let mut format_at = vec![None; chars];
    if def_name.is_some() {
        for span in spans {
            let start = span.start as usize;
            let end = start.saturating_add(span.len as usize).min(chars);
            for slot in format_at.iter_mut().take(end).skip(start) {
                *slot = Some(span.format);
            }
        }
    }
    let mut runs = Vec::new();
    let mut index = 0usize;
    while index < chars {
        let format = format_at[index];
        let mut next = index + 1;
        while next < chars && format_at[next] == format {
            next += 1;
        }
        let style = match (def_name, format) {
            (Some(name), Some(id)) => theme.resolve(name, repo.format(id)),
            _ => theme.text_style(DefaultStyle::Normal),
        };
        let len = byte_of[next] - byte_of[index];
        runs.push(run_from_style(len, &style, font, foreground));
        index = next;
    }
    runs
}

impl IntoElement for EditorSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style {
            flex_grow: 1.0,
            ..Default::default()
        };
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut gpui::App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let drawn = draw_editor(&mut self.editor, &self.root, bounds, window, cx);
        let view = self.editor.view();
        let line_height = self.editor.line_height();
        let editor = std::mem::replace(&mut self.editor, EditorElement::new(view, line_height));
        self.root.update(cx, |root, _| {
            root.restore_paint(editor, drawn);
        });
    }
}

pub(crate) struct DrawnFrame {
    pub column_width: Pixels,
    pub caret_bounds: Option<Bounds<Pixels>>,
    pub sample: FrameSample,
}

fn draw_editor(
    editor: &mut EditorElement,
    root: &Entity<WindowRoot>,
    mut bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> DrawnFrame {
    if f32::from(bounds.size.height) < 1.0 {
        bounds = Bounds::new(point(px(0.0), px(0.0)), window.viewport_size());
    }
    let started = std::time::Instant::now();
    let snap = root.read_with(cx, |root, app| root.snapshot(app));
    let line_height = editor.line_height();
    let fraction =
        (snap.scroll_y - f32::from(line_height) * snap.scroll.first_line as f32).max(0.0);
    let range = crate::layout::visible_line_range(
        bounds.size.height + px(fraction),
        line_height,
        snap.scroll,
        snap.line_count,
    );
    let misses: Vec<u32> = range
        .clone()
        .filter(|line| editor.cache_mut().get(*line, snap.revision).is_none())
        .collect();
    let last = range.end.saturating_sub(1);
    let plans = root.update(cx, |root, cx| root.highlight_lines(&misses, last, cx));
    let font_size = px(FONT_SIZE);
    let mut column_width = snap.column_width;
    if !snap.column_ready {
        column_width = measure_column(window, &snap.font, font_size, snap.colors.foreground);
    }
    for plan in &plans {
        if editor.cache_mut().get(plan.index, plan.revision).is_some() {
            continue;
        }
        let shaped = shape_plan(window, plan, font_size);
        editor.cache_mut().insert(plan.index, plan.revision, shaped);
    }
    if !range.is_empty() {
        editor
            .cache_mut()
            .retain_near(range.start, range.end, snap.revision);
    }

    let gutter = gutter_width(column_width, snap.line_count);
    window.handle_input(
        &snap.focus,
        gpui::ElementInputHandler::new(bounds, root.clone()),
        cx,
    );
    let mut caret_bounds = None;
    let mut painted = 0u32;
    let text_left = bounds.left() + gutter;
    for index in range.clone() {
        let y = bounds.top()
            + px(line_top(
                index,
                snap.scroll.first_line,
                fraction,
                line_height,
            ));
        if index == snap.caret_line {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), y),
                    size(bounds.size.width, line_height),
                ),
                snap.colors.current_line,
            ));
        }
        if let Some(shaped) = editor.cache_mut().get(index, snap.revision) {
            let origin = point(text_left - px(snap.scroll_x), y);
            if !shaped.text.is_empty() {
                let _ = shaped.paint_background(origin, line_height, window, cx);
                let _ = shaped.paint(origin, line_height, window, cx);
            }
            if index == snap.caret_line {
                let caret_x = origin.x + shaped.x_for_index(snap.caret_byte);
                caret_bounds = Some(paint_caret_and_ime(
                    window,
                    cx,
                    caret_x.max(text_left),
                    y,
                    line_height,
                    &snap,
                    font_size,
                ));
            }
        }
        painted += 1;
    }
    window.paint_quad(fill(
        Bounds::new(
            point(bounds.left(), bounds.top()),
            size(gutter, bounds.size.height),
        ),
        snap.colors.gutter,
    ));
    for index in range {
        let y = bounds.top()
            + px(line_top(
                index,
                snap.scroll.first_line,
                fraction,
                line_height,
            ));
        paint_line_number(
            window,
            cx,
            &snap.font,
            GutterLabel {
                index,
                left: bounds.left(),
                y,
                gutter,
                line_height,
                font_size,
                color: snap.colors.gutter_text,
            },
        );
    }
    let paint_micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    DrawnFrame {
        column_width,
        caret_bounds,
        sample: FrameSample {
            frame: snap.frame.saturating_add(1),
            paint_micros,
            first_line: snap.scroll.first_line,
            lines_painted: painted,
        },
    }
}

fn line_top(index: u32, first: u32, fraction: f32, line_height: Pixels) -> f32 {
    let rows = index as f32 - first as f32;
    rows * f32::from(line_height) - fraction
}

fn shape_plan(window: &mut Window, plan: &LinePlan, font_size: Pixels) -> ShapedLine {
    if plan.text.is_empty() || plan.runs.is_empty() {
        return ShapedLine::default();
    }
    debug_assert!(!plan.text.contains('\n'));
    let text: SharedString = plan.text.clone().into();
    window
        .text_system()
        .shape_line(text, font_size, &plan.runs, None)
}

fn measure_column(window: &mut Window, font: &Font, font_size: Pixels, color: Hsla) -> Pixels {
    let run = TextRun {
        len: 1,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line("0".into(), font_size, &[run], None);
    let width = f32::from(shaped.width);
    if width > 0.0 { px(width) } else { px(8.0) }
}

struct GutterLabel {
    index: u32,
    left: Pixels,
    y: Pixels,
    gutter: Pixels,
    line_height: Pixels,
    font_size: Pixels,
    color: Hsla,
}

fn paint_line_number(window: &mut Window, cx: &mut gpui::App, font: &Font, label: GutterLabel) {
    let text = label.index.saturating_add(1).to_string();
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: label.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text.into(), label.font_size, &[run], None);
    let x = label.left + label.gutter - px(8.0) - shaped.width;
    let _ = shaped.paint(point(x, label.y), label.line_height, window, cx);
}

fn paint_caret_and_ime(
    window: &mut Window,
    cx: &mut gpui::App,
    x: Pixels,
    y: Pixels,
    line_height: Pixels,
    snap: &crate::session::ViewSnap,
    font_size: Pixels,
) -> Bounds<Pixels> {
    let caret = Bounds::new(point(x, y), size(px(1.5), line_height));
    if snap.ime_text.is_empty() {
        window.paint_quad(fill(caret, snap.colors.foreground));
        return caret;
    }
    let runs = marked_runs(&snap.ime_text, snap.marked.clone(), &snap.font, snap.colors);
    let shaped =
        window
            .text_system()
            .shape_line(snap.ime_text.clone().into(), font_size, &runs, None);
    let _ = shaped.paint_background(point(x, y), line_height, window, cx);
    let _ = shaped.paint(point(x, y), line_height, window, cx);
    let caret_x = x + shaped.x_for_index(snap.marked.as_ref().map(|range| range.end).unwrap_or(0));
    let caret = Bounds::new(point(caret_x, y), size(px(1.5), line_height));
    window.paint_quad(fill(caret, snap.colors.foreground));
    Bounds::new(point(x, y), size(shaped.width.max(px(4.0)), line_height))
}

fn marked_runs(
    text: &str,
    marked: Option<Range<usize>>,
    font: &Font,
    colors: Colors,
) -> Vec<TextRun> {
    let mark = marked.unwrap_or(0..0);
    let start = mark.start.min(text.len());
    let end = mark.end.min(text.len()).max(start);
    let mut runs = Vec::new();
    push_run(&mut runs, start, font, colors.foreground, None, false);
    push_run(
        &mut runs,
        end - start,
        font,
        colors.foreground,
        Some(colors.marked),
        true,
    );
    push_run(
        &mut runs,
        text.len() - end,
        font,
        colors.foreground,
        None,
        false,
    );
    if runs.is_empty() {
        push_run(&mut runs, text.len(), font, colors.foreground, None, false);
    }
    runs
}

fn push_run(
    runs: &mut Vec<TextRun>,
    len: usize,
    font: &Font,
    color: Hsla,
    background: Option<Hsla>,
    underline: bool,
) {
    if len == 0 {
        return;
    }
    runs.push(TextRun {
        len,
        font: font.clone(),
        color,
        background_color: background,
        underline: underline.then_some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: None,
    });
}

fn gutter_width(column_width: Pixels, line_count: u32) -> Pixels {
    column_width * (digit_count(line_count.max(1)) as f32) + px(16.0)
}

fn digit_count(value: u32) -> u32 {
    let mut digits = 1u32;
    let mut scale = 10u32;
    while value >= scale {
        digits += 1;
        if scale > u32::MAX / 10 {
            break;
        }
        scale *= 10;
    }
    digits
}

fn run_from_style(len: usize, style: &ThemeStyle, font: &Font, fallback: Hsla) -> TextRun {
    let mut face = font.clone();
    if style.bold {
        face = face.bold();
    }
    if style.italic {
        face = face.italic();
    }
    let color = style.fg.map(hsla_from_rgba).unwrap_or(fallback);
    TextRun {
        len,
        font: face,
        color,
        background_color: style.bg.map(hsla_from_rgba),
        underline: style.underline.then_some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: style.strikeout.then_some(gpui::StrikethroughStyle {
            thickness: px(1.0),
            color: Some(color),
        }),
    }
}

fn color_or(theme: &Theme, role: EditorColor, fallback: impl Into<Hsla>) -> Hsla {
    theme
        .editor_color(role)
        .map(hsla_from_rgba)
        .unwrap_or_else(|| fallback.into())
}

fn hsla_from_rgba(color: Rgba) -> Hsla {
    gpui::rgba(u32::from_be_bytes([color.0, color.1, color.2, color.3])).into()
}
