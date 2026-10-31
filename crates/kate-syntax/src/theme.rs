// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! KSyntaxHighlighting JSON themes and format-to-style resolution.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::definition::{DefaultStyle, Format, Rgb};

/// One color. The channel order is red, green, blue, alpha.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

/// Final appearance of one span.
///
/// Every field is resolved. The renderer does no theme lookup.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub fg: Option<Rgba>,
    pub bg: Option<Rgba>,
    pub selected_fg: Option<Rgba>,
    pub selected_bg: Option<Rgba>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

/// One key from the `editor-colors` object.
///
/// The names match `KSyntaxHighlighting::Theme::EditorColorRole`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum EditorColor {
    BackgroundColor,
    TextSelection,
    CurrentLine,
    SearchHighlight,
    ReplaceHighlight,
    BracketMatching,
    TabMarker,
    SpellChecking,
    IndentationLine,
    IconBorder,
    CodeFolding,
    LineNumbers,
    CurrentLineNumber,
    WordWrapMarker,
    ModifiedLines,
    SavedLines,
    Separator,
    MarkBookmark,
    MarkBreakpointActive,
    MarkBreakpointReached,
    MarkBreakpointDisabled,
    MarkExecution,
    MarkWarning,
    MarkError,
    TemplateBackground,
    TemplatePlaceholder,
    TemplateFocusedPlaceholder,
    TemplateReadOnlyPlaceholder,
}

/// A loaded theme file.
pub struct Theme {
    pub name: String,
    pub revision: u32,
    pub license: String,
    text_styles: [Style; 31],
    editor_colors: HashMap<EditorColor, Rgba>,
    custom_styles: HashMap<String, HashMap<String, StyleSpec>>,
}

impl Theme {
    /// Parse one theme file.
    ///
    /// `json` is the file text. The function returns an error for invalid
    /// JSON, an unknown style name, or a color that is not valid hex.
    pub fn from_json(json: &str) -> Result<Theme, ThemeError> {
        let raw: RawTheme = serde_json::from_str(json).map_err(ThemeError::Json)?;
        Ok(Theme {
            name: raw.metadata.name,
            revision: raw.metadata.revision,
            license: raw.metadata.license,
            text_styles: read_text_styles(&raw.text_styles)?,
            editor_colors: read_editor_colors(&raw.editor_colors)?,
            custom_styles: read_custom_styles(&raw.custom_styles)?,
        })
    }

    /// Return the theme default for `style`.
    pub fn text_style(&self, style: DefaultStyle) -> Style {
        self.text_styles[style_index(style)]
    }

    /// Return the editor color for `color` when the file sets it.
    pub fn editor_color(&self, color: EditorColor) -> Option<Rgba> {
        self.editor_colors.get(&color).copied()
    }

    /// Resolve `format` for `definition`.
    ///
    /// A per-definition custom style overrides the format. The format
    /// overrides the theme default for `format.style`.
    pub fn resolve(&self, definition: &str, format: &Format) -> Style {
        let mut style = self.text_style(format.style);
        overlay_format(&mut style, format);
        if let Some(custom) = self.custom_for(definition, &format.name) {
            custom.overlay(&mut style);
        }
        style
    }

    fn custom_for(&self, definition: &str, format: &str) -> Option<&StyleSpec> {
        self.custom_styles.get(definition)?.get(format)
    }
}

/// A theme file that this crate cannot load.
#[derive(Debug)]
pub enum ThemeError {
    /// The text is not valid JSON, or a required field has the wrong type.
    Json(serde_json::Error),
    /// `text-styles` or `editor-colors` names a key this crate does not know.
    UnknownStyle(String),
    /// A color string is not a hex color. The string is the file value.
    BadColor(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(err) => write!(f, "invalid theme JSON: {err}"),
            Self::UnknownStyle(name) => write!(f, "unknown theme style {name}"),
            Self::BadColor(value) => write!(f, "invalid theme color {value}"),
        }
    }
}

impl std::error::Error for ThemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(err) => Some(err),
            Self::UnknownStyle(_) | Self::BadColor(_) => None,
        }
    }
}

/// One theme object before absent fields are filled.
///
/// A font flag stays `None` when the file omits it. `Style` cannot store that
/// distinction because its flags are `bool`.
struct StyleSpec {
    fg: Option<Rgba>,
    bg: Option<Rgba>,
    selected_fg: Option<Rgba>,
    selected_bg: Option<Rgba>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strikeout: Option<bool>,
}

impl StyleSpec {
    fn into_style(self) -> Style {
        Style {
            fg: self.fg,
            bg: self.bg,
            selected_fg: self.selected_fg,
            selected_bg: self.selected_bg,
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            underline: self.underline.unwrap_or(false),
            strikeout: self.strikeout.unwrap_or(false),
        }
    }

    fn overlay(&self, style: &mut Style) {
        if let Some(color) = self.fg {
            style.fg = Some(color);
        }
        if let Some(color) = self.bg {
            style.bg = Some(color);
        }
        if let Some(color) = self.selected_fg {
            style.selected_fg = Some(color);
        }
        if let Some(color) = self.selected_bg {
            style.selected_bg = Some(color);
        }
        if let Some(value) = self.bold {
            style.bold = value;
        }
        if let Some(value) = self.italic {
            style.italic = value;
        }
        if let Some(value) = self.underline {
            style.underline = value;
        }
        if let Some(value) = self.strikeout {
            style.strikeout = value;
        }
    }
}

#[derive(Deserialize)]
struct RawTheme {
    metadata: RawMeta,
    #[serde(rename = "text-styles")]
    text_styles: Map<String, Value>,
    #[serde(rename = "editor-colors")]
    editor_colors: Map<String, Value>,
    #[serde(default, rename = "custom-styles")]
    custom_styles: Map<String, Value>,
}

#[derive(Deserialize)]
struct RawMeta {
    name: String,
    revision: u32,
    license: String,
}

const TEXT_STYLES: [(&str, DefaultStyle); 31] = [
    ("Normal", DefaultStyle::Normal),
    ("Keyword", DefaultStyle::Keyword),
    ("Function", DefaultStyle::Function),
    ("Variable", DefaultStyle::Variable),
    ("ControlFlow", DefaultStyle::ControlFlow),
    ("Operator", DefaultStyle::Operator),
    ("BuiltIn", DefaultStyle::BuiltIn),
    ("Extension", DefaultStyle::Extension),
    ("Preprocessor", DefaultStyle::Preprocessor),
    ("Attribute", DefaultStyle::Attribute),
    ("Char", DefaultStyle::Char),
    ("SpecialChar", DefaultStyle::SpecialChar),
    ("String", DefaultStyle::String),
    ("VerbatimString", DefaultStyle::VerbatimString),
    ("SpecialString", DefaultStyle::SpecialString),
    ("Import", DefaultStyle::Import),
    ("DataType", DefaultStyle::DataType),
    ("DecVal", DefaultStyle::DecVal),
    ("BaseN", DefaultStyle::BaseN),
    ("Float", DefaultStyle::Float),
    ("Constant", DefaultStyle::Constant),
    ("Comment", DefaultStyle::Comment),
    ("Documentation", DefaultStyle::Documentation),
    ("Annotation", DefaultStyle::Annotation),
    ("CommentVar", DefaultStyle::CommentVar),
    ("RegionMarker", DefaultStyle::RegionMarker),
    ("Information", DefaultStyle::Information),
    ("Warning", DefaultStyle::Warning),
    ("Alert", DefaultStyle::Alert),
    ("Error", DefaultStyle::Error),
    ("Others", DefaultStyle::Others),
];

const EDITOR_ROLES: [(&str, EditorColor); 28] = [
    ("BackgroundColor", EditorColor::BackgroundColor),
    ("TextSelection", EditorColor::TextSelection),
    ("CurrentLine", EditorColor::CurrentLine),
    ("SearchHighlight", EditorColor::SearchHighlight),
    ("ReplaceHighlight", EditorColor::ReplaceHighlight),
    ("BracketMatching", EditorColor::BracketMatching),
    ("TabMarker", EditorColor::TabMarker),
    ("SpellChecking", EditorColor::SpellChecking),
    ("IndentationLine", EditorColor::IndentationLine),
    ("IconBorder", EditorColor::IconBorder),
    ("CodeFolding", EditorColor::CodeFolding),
    ("LineNumbers", EditorColor::LineNumbers),
    ("CurrentLineNumber", EditorColor::CurrentLineNumber),
    ("WordWrapMarker", EditorColor::WordWrapMarker),
    ("ModifiedLines", EditorColor::ModifiedLines),
    ("SavedLines", EditorColor::SavedLines),
    ("Separator", EditorColor::Separator),
    ("MarkBookmark", EditorColor::MarkBookmark),
    ("MarkBreakpointActive", EditorColor::MarkBreakpointActive),
    ("MarkBreakpointReached", EditorColor::MarkBreakpointReached),
    (
        "MarkBreakpointDisabled",
        EditorColor::MarkBreakpointDisabled,
    ),
    ("MarkExecution", EditorColor::MarkExecution),
    ("MarkWarning", EditorColor::MarkWarning),
    ("MarkError", EditorColor::MarkError),
    ("TemplateBackground", EditorColor::TemplateBackground),
    ("TemplatePlaceholder", EditorColor::TemplatePlaceholder),
    (
        "TemplateFocusedPlaceholder",
        EditorColor::TemplateFocusedPlaceholder,
    ),
    (
        "TemplateReadOnlyPlaceholder",
        EditorColor::TemplateReadOnlyPlaceholder,
    ),
];

const LEGACY_EDITOR_ROLES: [(&str, EditorColor); 28] = [
    ("background-color", EditorColor::BackgroundColor),
    ("selection", EditorColor::TextSelection),
    ("current-line", EditorColor::CurrentLine),
    ("search-highlight", EditorColor::SearchHighlight),
    ("replace-highlight", EditorColor::ReplaceHighlight),
    ("bracket-matching", EditorColor::BracketMatching),
    ("tab-marker", EditorColor::TabMarker),
    ("spell-checking", EditorColor::SpellChecking),
    ("indentation-line", EditorColor::IndentationLine),
    ("icon-border", EditorColor::IconBorder),
    ("code-folding", EditorColor::CodeFolding),
    ("line-numbers", EditorColor::LineNumbers),
    ("current-line-number", EditorColor::CurrentLineNumber),
    ("word-wrap-marker", EditorColor::WordWrapMarker),
    ("modified-lines", EditorColor::ModifiedLines),
    ("saved-lines", EditorColor::SavedLines),
    ("separator", EditorColor::Separator),
    ("mark-bookmark", EditorColor::MarkBookmark),
    ("mark-breakpoint-active", EditorColor::MarkBreakpointActive),
    (
        "mark-breakpoint-reached",
        EditorColor::MarkBreakpointReached,
    ),
    (
        "mark-breakpoint-disabled",
        EditorColor::MarkBreakpointDisabled,
    ),
    ("mark-execution", EditorColor::MarkExecution),
    ("mark-warning", EditorColor::MarkWarning),
    ("mark-error", EditorColor::MarkError),
    ("template-background", EditorColor::TemplateBackground),
    ("template-placeholder", EditorColor::TemplatePlaceholder),
    (
        "template-focused-placeholder",
        EditorColor::TemplateFocusedPlaceholder,
    ),
    (
        "template-read-only-placeholder",
        EditorColor::TemplateReadOnlyPlaceholder,
    ),
];

fn style_index(style: DefaultStyle) -> usize {
    TEXT_STYLES
        .iter()
        .position(|(_, candidate)| *candidate == style)
        .expect("TEXT_STYLES lists every DefaultStyle")
}

fn read_text_styles(raw: &Map<String, Value>) -> Result<[Style; 31], ThemeError> {
    let mut styles = [Style::default(); 31];
    for (name, value) in raw {
        let index = TEXT_STYLES
            .iter()
            .position(|(key, _)| *key == name)
            .ok_or_else(|| ThemeError::UnknownStyle(name.clone()))?;
        styles[index] = read_spec(json_object(value)?)?.into_style();
    }
    Ok(styles)
}

fn read_custom_styles(
    raw: &Map<String, Value>,
) -> Result<HashMap<String, HashMap<String, StyleSpec>>, ThemeError> {
    let mut custom = HashMap::new();
    for (definition, formats_value) in raw {
        let mut by_format = HashMap::new();
        for (format_name, spec_value) in json_object(formats_value)? {
            by_format.insert(format_name.clone(), read_spec(json_object(spec_value)?)?);
        }
        custom.insert(definition.clone(), by_format);
    }
    Ok(custom)
}

fn json_object(value: &Value) -> Result<&Map<String, Value>, ThemeError> {
    value.as_object().ok_or_else(|| {
        match serde_json::from_value::<Map<String, Value>>(value.clone()) {
            Err(err) => ThemeError::Json(err),
            Ok(_) => ThemeError::UnknownStyle(value.to_string()),
        }
    })
}

fn read_editor_colors(raw: &Map<String, Value>) -> Result<HashMap<EditorColor, Rgba>, ThemeError> {
    reject_unknown_editor_keys(raw)?;
    let mut colors = HashMap::new();
    for (name, role) in EDITOR_ROLES {
        if let Some(value) = raw.get(name) {
            store_editor_color(&mut colors, role, value)?;
        }
    }
    if !raw.contains_key("BackgroundColor") {
        for (name, role) in LEGACY_EDITOR_ROLES {
            match raw.get(name) {
                Some(value) => store_editor_color(&mut colors, role, value)?,
                None => {
                    colors.remove(&role);
                }
            }
        }
    }
    Ok(colors)
}

fn reject_unknown_editor_keys(raw: &Map<String, Value>) -> Result<(), ThemeError> {
    for name in raw.keys() {
        if role_named(name, &EDITOR_ROLES).is_none()
            && role_named(name, &LEGACY_EDITOR_ROLES).is_none()
        {
            return Err(ThemeError::UnknownStyle(name.clone()));
        }
    }
    Ok(())
}

fn role_named(name: &str, table: &[(&str, EditorColor)]) -> Option<EditorColor> {
    table
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, role)| *role)
}

fn store_editor_color(
    colors: &mut HashMap<EditorColor, Rgba>,
    role: EditorColor,
    value: &Value,
) -> Result<(), ThemeError> {
    match read_optional_color(value)? {
        Some(color) => {
            colors.insert(role, color);
        }
        None => {
            colors.remove(&role);
        }
    }
    Ok(())
}

fn read_spec(obj: &Map<String, Value>) -> Result<StyleSpec, ThemeError> {
    Ok(StyleSpec {
        fg: read_color_field(obj, "text-color")?,
        bg: read_color_field(obj, "background-color")?,
        selected_fg: read_color_field(obj, "selected-text-color")?,
        selected_bg: read_color_field(obj, "selected-background-color")?,
        bold: read_flag(obj, "bold")?,
        italic: read_flag(obj, "italic")?,
        underline: read_flag(obj, "underline")?,
        strikeout: read_flag(obj, "strike-through")?,
    })
}

fn read_color_field(obj: &Map<String, Value>, key: &str) -> Result<Option<Rgba>, ThemeError> {
    match obj.get(key) {
        Some(value) => read_optional_color(value),
        None => Ok(None),
    }
}

fn read_optional_color(value: &Value) -> Result<Option<Rgba>, ThemeError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(unset_if_transparent_black(parse_hex_color(text)?)),
        other => Err(ThemeError::BadColor(other.to_string())),
    }
}

fn read_flag(obj: &Map<String, Value>, key: &str) -> Result<Option<bool>, ThemeError> {
    match obj.get(key) {
        None => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(ThemeError::Json),
    }
}

fn unset_if_transparent_black(color: Rgba) -> Option<Rgba> {
    if color == Rgba(0, 0, 0, 0) {
        None
    } else {
        Some(color)
    }
}

fn parse_hex_color(text: &str) -> Result<Rgba, ThemeError> {
    let Some(hex) = text.strip_prefix('#') else {
        return Err(ThemeError::BadColor(text.to_string()));
    };
    if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ThemeError::BadColor(text.to_string()));
    }
    let Some((alpha, red, green, blue)) = decode_channels(hex.as_bytes()) else {
        return Err(ThemeError::BadColor(text.to_string()));
    };
    Ok(Rgba(
        high_byte(red, text)?,
        high_byte(green, text)?,
        high_byte(blue, text)?,
        high_byte(alpha, text)?,
    ))
}

/// Qt `get_hex_rgb` lengths. Each channel is 16-bit. The high byte is the color.
fn decode_channels(bytes: &[u8]) -> Option<(u32, u32, u32, u32)> {
    let channels = match bytes.len() {
        12 => (
            65535,
            hex_digits(&bytes[0..4]),
            hex_digits(&bytes[4..8]),
            hex_digits(&bytes[8..12]),
        ),
        9 => {
            let mut red = hex_digits(&bytes[0..3]);
            let mut green = hex_digits(&bytes[3..6]);
            let mut blue = hex_digits(&bytes[6..9]);
            red = (red << 4) | (red >> 8);
            green = (green << 4) | (green >> 8);
            blue = (blue << 4) | (blue >> 8);
            (65535, red, green, blue)
        }
        8 => (
            hex_digits(&bytes[0..2]) * 0x101,
            hex_digits(&bytes[2..4]) * 0x101,
            hex_digits(&bytes[4..6]) * 0x101,
            hex_digits(&bytes[6..8]) * 0x101,
        ),
        6 => (
            65535,
            hex_digits(&bytes[0..2]) * 0x101,
            hex_digits(&bytes[2..4]) * 0x101,
            hex_digits(&bytes[4..6]) * 0x101,
        ),
        3 => (
            65535,
            hex_digits(&bytes[0..1]) * 0x1111,
            hex_digits(&bytes[1..2]) * 0x1111,
            hex_digits(&bytes[2..3]) * 0x1111,
        ),
        _ => return None,
    };
    Some(channels)
}

fn hex_digits(bytes: &[u8]) -> u32 {
    let mut value = 0u32;
    for byte in bytes {
        value = (value << 4) | u32::from(hex_value(*byte));
    }
    value
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn high_byte(value: u32, text: &str) -> Result<u8, ThemeError> {
    u8::try_from(value >> 8).map_err(|_| ThemeError::BadColor(text.to_string()))
}

fn overlay_format(style: &mut Style, format: &Format) {
    if let Some(rgb) = format.color {
        style.fg = Some(opaque(rgb));
    }
    if let Some(rgb) = format.selected_color {
        style.selected_fg = Some(opaque(rgb));
    }
    if let Some(rgb) = format.background {
        style.bg = Some(opaque(rgb));
    }
    if let Some(rgb) = format.selected_background {
        style.selected_bg = Some(opaque(rgb));
    }
    if let Some(value) = format.bold {
        style.bold = value;
    }
    if let Some(value) = format.italic {
        style.italic = value;
    }
    if let Some(value) = format.underline {
        style.underline = value;
    }
    if let Some(value) = format.strikeout {
        style.strikeout = value;
    }
}

fn opaque(rgb: Rgb) -> Rgba {
    Rgba(rgb.0, rgb.1, rgb.2, 255)
}

#[cfg(test)]
mod tests {
    use crate::definition::{DefaultStyle, Format, Rgb};

    use super::{EditorColor, Rgba, Style, Theme, ThemeError};

    const BREEZE_LIGHT: &str = include_str!("../themes/breeze-light.theme");
    const BREEZE_DARK: &str = include_str!("../themes/breeze-dark.theme");

    fn keyword_format(
        name: &str,
        color: Option<Rgb>,
        italic: Option<bool>,
        strikeout: Option<bool>,
    ) -> Format {
        Format {
            name: name.to_string(),
            style: DefaultStyle::Keyword,
            color,
            selected_color: None,
            background: None,
            selected_background: None,
            bold: None,
            italic,
            underline: None,
            strikeout,
            spell_check: true,
        }
    }

    #[test]
    fn breeze_light_and_dark_load() {
        let light = Theme::from_json(BREEZE_LIGHT).expect("breeze light parses");
        let dark = Theme::from_json(BREEZE_DARK).expect("breeze dark parses");

        assert_eq!(light.name, "Breeze Light");
        assert_eq!(light.revision, 10);
        assert_eq!(light.license, "SPDX-License-Identifier: MIT");
        assert_eq!(dark.name, "Breeze Dark");
        assert_eq!(dark.revision, 11);
        assert_eq!(dark.license, "SPDX-License-Identifier: MIT");

        assert!(BREEZE_LIGHT.contains("\"text-color\" : \"#1f1c1b\""));
        assert_eq!(
            light.text_style(DefaultStyle::Keyword).fg,
            Some(Rgba(0x1f, 0x1c, 0x1b, 0xff))
        );
        assert!(light.text_style(DefaultStyle::Keyword).bold);
        assert!(!light.text_style(DefaultStyle::Normal).bold);

        assert!(BREEZE_DARK.contains("\"text-color\" : \"#cfcfc2\""));
        assert_eq!(
            dark.text_style(DefaultStyle::Keyword).fg,
            Some(Rgba(0xcf, 0xcf, 0xc2, 0xff))
        );
        assert!(dark.text_style(DefaultStyle::Keyword).bold);

        assert_eq!(
            light.editor_color(EditorColor::BackgroundColor),
            Some(Rgba(0xff, 0xff, 0xff, 0xff))
        );
        assert_eq!(
            dark.editor_color(EditorColor::BackgroundColor),
            Some(Rgba(0x23, 0x26, 0x29, 0xff))
        );
    }

    #[test]
    fn resolution_order_is_custom_then_format_then_default() {
        let json = r##"{
            "metadata": {
                "name": "Fixture",
                "revision": 1,
                "license": "SPDX-License-Identifier: MIT"
            },
            "text-styles": {
                "Keyword": {
                    "text-color": "#111111",
                    "selected-text-color": "#222222",
                    "background-color": "#333333",
                    "selected-background-color": "#444444",
                    "bold": true,
                    "italic": false,
                    "underline": true,
                    "strike-through": false
                }
            },
            "editor-colors": { "BackgroundColor": "#ffffff" },
            "custom-styles": {
                "rust": {
                    "Keyword": { "text-color": "#aaaaaa", "bold": false }
                }
            }
        }"##;
        let theme = Theme::from_json(json).expect("fixture parses");
        let default_keyword = theme.text_style(DefaultStyle::Keyword);

        let custom = keyword_format(
            "Keyword",
            Some(Rgb(0x00, 0xff, 0x00)),
            Some(true),
            Some(true),
        );
        assert_eq!(
            theme.resolve("rust", &custom),
            Style {
                fg: Some(Rgba(0xaa, 0xaa, 0xaa, 0xff)),
                bg: Some(Rgba(0x33, 0x33, 0x33, 0xff)),
                selected_fg: Some(Rgba(0x22, 0x22, 0x22, 0xff)),
                selected_bg: Some(Rgba(0x44, 0x44, 0x44, 0xff)),
                bold: false,
                italic: true,
                underline: true,
                strikeout: true,
            }
        );

        let same_name_other_def = keyword_format("Keyword", None, Some(true), None);
        let mut over_default = default_keyword;
        over_default.italic = true;
        assert_eq!(theme.resolve("other", &same_name_other_def), over_default);
        assert_ne!(
            theme.resolve("other", &same_name_other_def),
            default_keyword
        );

        let other_format_same_def = keyword_format("Other", None, Some(true), None);
        assert_eq!(theme.resolve("rust", &other_format_same_def), over_default);

        let plain = keyword_format("Keyword", None, None, None);
        assert_eq!(theme.resolve("other", &plain), default_keyword);
    }

    #[test]
    fn bad_color_is_an_error_not_black() {
        let json = r##"{
            "metadata": {"name": "bad", "revision": 1, "license": "MIT"},
            "text-styles": {"Keyword": {"text-color": "#GG0000"}},
            "editor-colors": {}
        }"##;
        match Theme::from_json(json) {
            Err(ThemeError::BadColor(value)) => assert_eq!(value, "#GG0000"),
            Err(other) => panic!("expected BadColor, got {other}"),
            Ok(_) => panic!("bad color returned a theme"),
        }
    }
}
