// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! KSyntaxHighlighting JSON themes and format-to-style resolution.

use std::collections::HashMap;

use crate::definition::{DefaultStyle, Format};

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
    custom_styles: HashMap<(String, String), Style>,
}

impl Theme {
    /// Parse one theme file.
    ///
    /// `json` is the file text. The function returns an error for invalid
    /// JSON, an unknown style name, or a color that is not valid hex.
    pub fn from_json(json: &str) -> Result<Theme, ThemeError> {
        let _ = json;
        Err(ThemeError::BadColor(String::new()))
    }

    /// Return the theme default for `style`.
    pub fn text_style(&self, style: DefaultStyle) -> Style {
        let _ = style;
        self.text_styles[0]
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
        let _ = definition;
        let _ = self.custom_styles.len();
        self.text_style(format.style)
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
