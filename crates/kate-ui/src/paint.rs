// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(target_os = "macos")]

//! Paint one view.
//!
//! The shape cache stays on the window root.
//! Line numbers are shaped once per number and theme.

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    Bounds, Element, ElementId, Entity, Font, FontFallbacks, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextRun,
    UnderlineStyle, Window, fill, point, px, relative, size,
};
use kate_syntax::{DefaultStyle, EditorColor, Rgba, Span, Style as ThemeStyle, Theme};
use kate_text::{Revision, TextBuffer};

use crate::cache::{KeyedCache, theme_bits};
use crate::session::WindowRoot;
use crate::{FrameSample, ShapedLineCache};

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
        let mut loan = self.root.update(cx, |root, _| CacheLoan {
            shaped: std::mem::take(&mut root.shaped),
            gutter: std::mem::take(&mut root.gutter),
        });
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            draw_editor(
                &mut loan.shaped,
                &mut loan.gutter,
                &self.root,
                bounds,
                window,
                cx,
            )
        }));
        let drawn = match drawn {
            Ok(drawn) => drawn,
            Err(payload) => {
                self.root.update(cx, |root, _| {
                    root.shaped = std::mem::take(&mut loan.shaped);
                    root.gutter = std::mem::take(&mut loan.gutter);
                });
                std::panic::resume_unwind(payload);
            }
        };
        self.root.update(cx, |root, cx| {
            root.shaped = std::mem::take(&mut loan.shaped);
            root.gutter = std::mem::take(&mut loan.gutter);
            root.apply_frame(drawn, window, cx);
        });
    }
}

/// Caches borrowed from the window for one paint.
///
/// `paint` puts them back on both the normal path and a panic path.
struct CacheLoan {
    shaped: ShapedLineCache,
    gutter: KeyedCache<ShapedLine>,
}

pub(crate) struct DrawnFrame {
    pub column_width: Pixels,
    pub caret_bounds: Option<Bounds<Pixels>>,
    pub want_catch_up: bool,
    pub sample: FrameSample,
}

fn draw_editor(
    shaped: &mut ShapedLineCache,
    gutter_cache: &mut KeyedCache<ShapedLine>,
    root: &Entity<WindowRoot>,
    mut bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut gpui::App,
) -> DrawnFrame {
    if f32::from(bounds.size.height) < 1.0 {
        bounds = Bounds::new(point(px(0.0), px(0.0)), window.viewport_size());
    }
    let started = Instant::now();
    let snap = root.read_with(cx, |root, app| root.snapshot(app));
    let line_height = snap.line_height;
    let fraction =
        (snap.scroll_y - f32::from(line_height) * snap.scroll.first_line as f32).max(0.0);
    let range = crate::layout::visible_line_range(
        bounds.size.height + px(fraction),
        line_height,
        snap.scroll,
        snap.line_count,
    );
    let highlight_started = Instant::now();
    let prep = if range.is_empty() {
        None
    } else {
        let last = range.end - 1;
        Some(root.update(cx, |root, cx| root.prepare_highlight(range.start, last, cx)))
    };
    if let Some(prep) = prep.as_ref() {
        shaped.invalidate_range(prep.rebuild.start, prep.rebuild.end, snap.revision);
    }
    let plain_from = prep.as_ref().and_then(|prep| prep.plain_from);
    let want_catch_up = prep.as_ref().is_some_and(|prep| prep.want_catch_up);
    let misses: Vec<u32> = range
        .clone()
        .filter(|line| shaped.get(*line, snap.revision).is_none())
        .collect();
    let plans = root.update(cx, |root, cx| root.plans_for(&misses, plain_from, cx));
    let highlight_micros = micros(highlight_started.elapsed());
    let font_size = px(FONT_SIZE);
    let mut shape_micros = 0u64;
    let mut column_width = snap.column_width;
    if !snap.column_ready {
        let shape_started = Instant::now();
        column_width = measure_column(window, &snap.font, font_size, snap.colors.foreground);
        shape_micros = shape_micros.saturating_add(micros(shape_started.elapsed()));
    }
    let shape_started = Instant::now();
    for plan in &plans {
        if shaped.get(plan.index, plan.revision).is_some() {
            continue;
        }
        let line = shape_plan(window, plan, font_size);
        shaped.insert(plan.index, plan.revision, line);
    }
    shape_micros = shape_micros.saturating_add(micros(shape_started.elapsed()));
    if !range.is_empty() {
        shaped.retain_near(range.start, range.end, snap.revision);
    }

    let gutter = gutter_width(column_width, snap.line_count);
    let draw_started = Instant::now();
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
        if let Some(line) = shaped.get(index, snap.revision) {
            let origin = point(text_left - px(snap.scroll_x), y);
            if !line.text.is_empty() {
                let _ = line.paint_background(origin, line_height, window, cx);
                let _ = line.paint(origin, line_height, window, cx);
            }
            if index == snap.caret_line {
                let caret_x = origin.x + line.x_for_index(snap.caret_byte);
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
    let mut draw_micros = micros(draw_started.elapsed());
    for index in range {
        let y = bounds.top()
            + px(line_top(
                index,
                snap.scroll.first_line,
                fraction,
                line_height,
            ));
        let (label_shape, label_draw) = paint_line_number(
            window,
            cx,
            gutter_cache,
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
        shape_micros = shape_micros.saturating_add(label_shape);
        draw_micros = draw_micros.saturating_add(label_draw);
    }
    let paint_micros = micros(started.elapsed());
    DrawnFrame {
        column_width,
        caret_bounds,
        want_catch_up,
        sample: FrameSample {
            frame: snap.frame.saturating_add(1),
            paint_micros,
            first_line: snap.scroll.first_line,
            lines_painted: painted,
            highlight_micros,
            shape_micros,
            draw_micros,
        },
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
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

fn paint_line_number(
    window: &mut Window,
    cx: &mut gpui::App,
    cache: &mut KeyedCache<ShapedLine>,
    font: &Font,
    label: GutterLabel,
) -> (u64, u64) {
    let number = label.index.saturating_add(1);
    let rgb = label.color.to_rgb();
    let theme = theme_bits(rgb.r, rgb.g, rgb.b, rgb.a);
    let (shaped, shape_micros) = if let Some(hit) = cache.get(number, theme).cloned() {
        (hit, 0)
    } else {
        let started = Instant::now();
        let text = number.to_string();
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
        cache.insert(number, theme, shaped.clone());
        (shaped, micros(started.elapsed()))
    };
    let draw_started = Instant::now();
    let x = label.left + label.gutter - px(8.0) - shaped.width;
    let _ = shaped.paint(point(x, label.y), label.line_height, window, cx);
    (shape_micros, micros(draw_started.elapsed()))
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
