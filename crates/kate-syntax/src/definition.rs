// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Syntax definition data. The loader in `repository` fills these values.

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DefId(pub u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ContextId(pub u32); // global across the repository

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FormatId(pub u16); // global

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeywordListId(pub u16);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FoldId(pub u16);

pub struct Definition {
    pub name: String,
    pub section: String,
    pub version: u32,
    pub priority: i32,
    pub hidden: bool,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
    pub case_sensitive_keywords: bool,
    pub indenter: Option<String>,
    pub word_delimiters: WordDelimiters,
    pub comments: Vec<CommentStyle>,
    pub indentation_based_folding: bool,
    pub initial_context: ContextId,
    pub contexts: std::ops::Range<u32>, // this definition's slice of Repository::contexts
    pub formats: std::ops::Range<u16>,
    pub license: String, // verbatim from the XML `license` attribute; vendoring policy depends on it
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordDelimiters {
    pub delimiters: Vec<char>,
    pub weak: Vec<char>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentStyle {
    SingleLine {
        start: String,
        position: CommentPosition,
    },
    MultiLine {
        start: String,
        end: String,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CommentPosition {
    Anywhere,
    AfterWhitespace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub name: String,
    pub attribute: FormatId,
    pub rules: Vec<Rule>,
    pub line_end: ContextSwitch,
    pub line_empty: ContextSwitch,
    pub fallthrough: Option<ContextSwitch>,
    pub no_indentation_based_folding: bool,
    pub stop_empty_line_context_switch_loop: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ContextSwitch {
    Stay,
    Pop(u8),
    Push(ContextId),
    PopPush(u8, ContextId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub kind: RuleKind,
    pub attribute: Option<FormatId>,
    pub switch: ContextSwitch,
    pub lookahead: bool,
    pub first_non_space: bool,
    pub column: Option<u32>,
    pub begin_region: Option<FoldId>,
    pub end_region: Option<FoldId>,
    pub dynamic: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleKind {
    AnyChar(Vec<char>),
    DetectChar(char),
    Detect2Chars(char, char),
    DetectIdentifier,
    DetectSpaces,
    Float,
    Int,
    HlCChar,
    HlCHex,
    HlCOct,
    HlCStringChar,
    Keyword {
        list: KeywordListId,
        insensitive: bool,
    },
    LineContinue(char),
    RangeDetect(char, char),
    RegExpr {
        pattern: String,
        insensitive: bool,
        minimal: bool,
    }, // compiled lazily in PKT-0008
    StringDetect {
        text: String,
        insensitive: bool,
    },
    WordDetect {
        word: String,
        insensitive: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeywordList {
    pub name: String,
    pub words: Vec<String>, // sorted, deduplicated; includes resolved `<include>`
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Format {
    pub name: String,
    pub style: DefaultStyle,
    pub color: Option<Rgb>,
    pub selected_color: Option<Rgb>,
    pub background: Option<Rgb>,
    pub selected_background: Option<Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikeout: Option<bool>,
    pub spell_check: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The 30 default text styles of KSyntaxHighlighting (dsNormal .. dsOthers), same order.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DefaultStyle {
    Normal,
    Keyword,
    Function,
    Variable,
    ControlFlow,
    Operator,
    BuiltIn,
    Extension,
    Preprocessor,
    Attribute,
    Char,
    SpecialChar,
    String,
    VerbatimString,
    SpecialString,
    Import,
    DataType,
    DecVal,
    BaseN,
    Float,
    Constant,
    Comment,
    Documentation,
    Annotation,
    CommentVar,
    RegionMarker,
    Information,
    Warning,
    Alert,
    Error,
    Others,
}

impl DefaultStyle {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "dsNormal" => Self::Normal,
            "dsKeyword" => Self::Keyword,
            "dsFunction" => Self::Function,
            "dsVariable" => Self::Variable,
            "dsControlFlow" => Self::ControlFlow,
            "dsOperator" => Self::Operator,
            "dsBuiltIn" => Self::BuiltIn,
            "dsExtension" => Self::Extension,
            "dsPreprocessor" => Self::Preprocessor,
            "dsAttribute" => Self::Attribute,
            "dsChar" => Self::Char,
            "dsSpecialChar" => Self::SpecialChar,
            "dsString" => Self::String,
            "dsVerbatimString" => Self::VerbatimString,
            "dsSpecialString" => Self::SpecialString,
            "dsImport" => Self::Import,
            "dsDataType" => Self::DataType,
            "dsDecVal" => Self::DecVal,
            "dsBaseN" => Self::BaseN,
            "dsFloat" => Self::Float,
            "dsConstant" => Self::Constant,
            "dsComment" => Self::Comment,
            "dsDocumentation" => Self::Documentation,
            "dsAnnotation" => Self::Annotation,
            "dsCommentVar" => Self::CommentVar,
            "dsRegionMarker" => Self::RegionMarker,
            "dsInformation" => Self::Information,
            "dsWarning" => Self::Warning,
            "dsAlert" => Self::Alert,
            "dsError" => Self::Error,
            "dsOthers" => Self::Others,
            _ => return None,
        })
    }
}
