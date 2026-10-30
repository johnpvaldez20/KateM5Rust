// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! XML reader for one KSyntaxHighlighting definition.
//! Cross-definition names stay as text until `Repository::link`.

use std::collections::HashMap;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::QName;
use quick_xml::reader::Reader;

use crate::definition::{CommentPosition, CommentStyle, DefaultStyle, Format, Rgb};
use crate::repository::LoadError;

pub(crate) struct ParsedDef {
    pub name: String,
    pub section: String,
    pub version: u32,
    pub priority: i32,
    pub hidden: bool,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
    pub case_sensitive: bool,
    pub indenter: Option<String>,
    pub additional_delimiters: String,
    pub weak_delimiters: String,
    pub comments: Vec<CommentStyle>,
    pub indentation_based_folding: bool,
    pub license: String,
    pub lists: Vec<ParsedList>,
    pub contexts: Vec<ParsedCtx>,
    pub formats: Vec<Format>,
}

pub(crate) struct ParsedList {
    pub name: String,
    pub words: Vec<String>,
    pub includes: Vec<String>,
}

pub(crate) struct ParsedCtx {
    pub name: String,
    pub attribute: String,
    pub line_end: String,
    pub line_empty: Option<String>,
    pub fallthrough: Option<String>,
    pub no_indentation_based_folding: bool,
    pub stop_empty_line_context_switch_loop: bool,
    pub items: Vec<PendingItem>,
}

pub(crate) enum PendingItem {
    Rule(PendingRule),
    Include {
        context: String,
        include_attrib: bool,
    },
}

pub(crate) struct PendingRule {
    pub kind: PendingKind,
    pub attribute: Option<String>,
    pub switch_raw: String,
    pub lookahead: bool,
    pub first_non_space: bool,
    pub column: Option<u32>,
    pub begin_region: Option<String>,
    pub end_region: Option<String>,
    pub dynamic: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum PendingKind {
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
        list: String,
        insensitive: Option<bool>,
    },
    LineContinue(char),
    RangeDetect(char, char),
    RegExpr {
        pattern: String,
        insensitive: bool,
        minimal: bool,
    },
    StringDetect {
        text: String,
        insensitive: bool,
    },
    WordDetect {
        word: String,
        insensitive: bool,
    },
}

pub(crate) fn parse_xml(xml: &str) -> Result<ParsedDef, LoadError> {
    let expanded = prepare_xml(xml)?;
    let mut cursor = XmlCursor::new(&expanded);
    loop {
        match next_tag(&mut cursor)? {
            Tag::Start(name, attrs) if name == "language" => {
                return parse_language(&mut cursor, attrs);
            }
            Tag::Empty(name, _) if name == "language" => {
                return Err(cursor.err("empty language element"));
            }
            Tag::Eof => return Err(cursor.err("missing language element")),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::End(_) | Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

struct XmlCursor<'a> {
    reader: Reader<&'a [u8]>,
    xml: &'a str,
    def_name: Option<String>,
}

impl<'a> XmlCursor<'a> {
    fn new(xml: &'a str) -> Self {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(false);
        Self {
            reader,
            xml,
            def_name: None,
        }
    }

    fn line(&self) -> u64 {
        let offset = usize::try_from(self.reader.buffer_position()).unwrap_or(self.xml.len());
        let offset = offset.min(self.xml.len());
        self.xml[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u64
            + 1
    }

    fn err(&self, message: impl Into<String>) -> LoadError {
        LoadError::Xml {
            definition: self.def_name.clone(),
            line: self.line(),
            message: message.into(),
        }
    }

    fn next(&mut self) -> Result<Event<'a>, LoadError> {
        match self.reader.read_event() {
            Ok(event) => Ok(event),
            Err(err) => Err(self.err(err.to_string())),
        }
    }

    fn skip_body(&mut self, name: &str) -> Result<(), LoadError> {
        match self.reader.read_to_end(QName(name)) {
            Ok(_) => Ok(()),
            Err(err) => Err(self.err(err.to_string())),
        }
    }
}

enum Tag {
    Start(String, HashMap<String, String>),
    Empty(String, HashMap<String, String>),
    End(String),
    Eof,
    Other,
}

fn next_tag(cursor: &mut XmlCursor) -> Result<Tag, LoadError> {
    let built = match cursor.next()? {
        Event::Start(start) => tag_from(start, true),
        Event::Empty(start) => tag_from(start, false),
        Event::End(end) => Ok(Tag::End(end.local_name().into_inner().to_string())),
        Event::Eof => Ok(Tag::Eof),
        _ => Ok(Tag::Other),
    };
    match built {
        Ok(tag) => Ok(tag),
        Err(message) => Err(cursor.err(message)),
    }
}

fn tag_from(start: BytesStart, open: bool) -> Result<Tag, String> {
    let name = start.local_name().into_inner().to_string();
    let attrs = take_attrs(&start)?;
    if open {
        Ok(Tag::Start(name, attrs))
    } else {
        Ok(Tag::Empty(name, attrs))
    }
}

fn take_attrs(start: &BytesStart) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|err| err.to_string())?;
        let key = attr.key.local_name().into_inner().to_string();
        let value = attr
            .normalized_value(XmlVersion::Explicit1_0)
            .map_err(|err| err.to_string())?;
        map.insert(key, value.into_owned());
    }
    Ok(map)
}

fn parse_language(
    cursor: &mut XmlCursor,
    attrs: HashMap<String, String>,
) -> Result<ParsedDef, LoadError> {
    let mut parsed = language_shell(&attrs, cursor)?;
    cursor.def_name = Some(parsed.name.clone());
    loop {
        match next_tag(cursor)? {
            Tag::Start(name, _) if name == "highlighting" => {
                parse_highlighting(cursor, &mut parsed)?
            }
            Tag::Empty(name, _) if name == "highlighting" => {}
            Tag::Start(name, attrs) if name == "general" => {
                parse_general(cursor, &mut parsed, false, &attrs)?
            }
            Tag::Empty(name, attrs) if name == "general" => {
                parse_general(cursor, &mut parsed, true, &attrs)?
            }
            Tag::End(name) if name == "language" => return Ok(parsed),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::Eof => return Err(cursor.err("truncated language element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

fn language_shell(
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<ParsedDef, LoadError> {
    let name = required(attrs, "name", cursor)?.to_string();
    let section = required(attrs, "section", cursor)?.to_string();
    let version = required(attrs, "version", cursor)?;
    let version = version
        .parse::<u32>()
        .map_err(|_| cursor.err(format!("bad version {version}")))?;
    let priority = match attrs.get("priority") {
        None => 0,
        Some(value) => value
            .parse::<i32>()
            .map_err(|_| cursor.err(format!("bad priority {value}")))?,
    };
    let hidden = flag(attrs, "hidden", cursor)?;
    let extensions = split_list(attrs.get("extensions").map(String::as_str).unwrap_or(""));
    let mime_types = split_list(attrs.get("mimetype").map(String::as_str).unwrap_or(""));
    let indenter = attrs.get("indenter").and_then(|value| non_empty(value));
    let license = attrs.get("license").cloned().unwrap_or_default();
    Ok(ParsedDef {
        name,
        section,
        version,
        priority,
        hidden,
        extensions,
        mime_types,
        case_sensitive: true,
        indenter,
        additional_delimiters: String::new(),
        weak_delimiters: String::new(),
        comments: Vec::new(),
        indentation_based_folding: false,
        license,
        lists: Vec::new(),
        contexts: Vec::new(),
        formats: Vec::new(),
    })
}

fn parse_highlighting(cursor: &mut XmlCursor, parsed: &mut ParsedDef) -> Result<(), LoadError> {
    loop {
        match next_tag(cursor)? {
            Tag::End(name) if name == "highlighting" => return Ok(()),
            Tag::Eof => return Err(cursor.err("truncated highlighting element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            tag => highlighting_child(cursor, parsed, tag)?,
        }
    }
}

fn highlighting_child(
    cursor: &mut XmlCursor,
    parsed: &mut ParsedDef,
    tag: Tag,
) -> Result<(), LoadError> {
    match tag {
        Tag::Start(name, attrs) if name == "list" => parsed.lists.push(parse_list(cursor, attrs)?),
        Tag::Empty(name, attrs) if name == "list" => parsed.lists.push(empty_list(&attrs, cursor)?),
        Tag::Start(name, _) if name == "contexts" => parse_contexts(cursor, parsed)?,
        Tag::Start(name, _) if name == "itemDatas" => parse_item_datas(cursor, parsed)?,
        Tag::Start(name, _) => cursor.skip_body(&name)?,
        Tag::Empty(_, _) | Tag::Other | Tag::End(_) | Tag::Eof => {}
    }
    Ok(())
}

fn parse_general(
    cursor: &mut XmlCursor,
    parsed: &mut ParsedDef,
    is_empty: bool,
    _attrs: &HashMap<String, String>,
) -> Result<(), LoadError> {
    if is_empty {
        return Ok(());
    }
    loop {
        match next_tag(cursor)? {
            Tag::End(name) if name == "general" => return Ok(()),
            Tag::Eof => return Err(cursor.err("truncated general element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            tag => general_child(cursor, parsed, tag)?,
        }
    }
}

fn general_child(
    cursor: &mut XmlCursor,
    parsed: &mut ParsedDef,
    tag: Tag,
) -> Result<(), LoadError> {
    match tag {
        Tag::Start(name, attrs) if name == "keywords" => {
            apply_keywords(parsed, &attrs, cursor)?;
            cursor.skip_body(&name)?;
        }
        Tag::Empty(name, attrs) if name == "keywords" => apply_keywords(parsed, &attrs, cursor)?,
        Tag::Start(name, attrs) if name == "folding" => {
            apply_folding(parsed, &attrs, cursor)?;
            cursor.skip_body(&name)?;
        }
        Tag::Empty(name, attrs) if name == "folding" => apply_folding(parsed, &attrs, cursor)?,
        Tag::Start(name, _) if name == "comments" => parse_comments(cursor, parsed)?,
        Tag::Start(name, _) => cursor.skip_body(&name)?,
        Tag::Empty(_, _) | Tag::Other | Tag::End(_) | Tag::Eof => {}
    }
    Ok(())
}

fn apply_keywords(
    parsed: &mut ParsedDef,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<(), LoadError> {
    if let Some(value) = attrs.get("casesensitive") {
        parsed.case_sensitive = parse_bool(value).ok_or_else(|| cursor.err("bad casesensitive"))?;
    }
    if let Some(value) = attrs.get("additionalDeliminator") {
        parsed.additional_delimiters.push_str(value);
    }
    if let Some(value) = attrs.get("weakDeliminator") {
        parsed.weak_delimiters.push_str(value);
    }
    Ok(())
}

fn apply_folding(
    parsed: &mut ParsedDef,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<(), LoadError> {
    if let Some(value) = attrs.get("indentationsensitive") {
        parsed.indentation_based_folding =
            parse_bool(value).ok_or_else(|| cursor.err("bad indentationsensitive"))?;
    }
    Ok(())
}

fn parse_comments(cursor: &mut XmlCursor, parsed: &mut ParsedDef) -> Result<(), LoadError> {
    loop {
        match next_tag(cursor)? {
            Tag::Start(name, attrs) if name == "comment" => {
                parsed.comments.push(comment_style(&attrs, cursor)?);
                cursor.skip_body(&name)?;
            }
            Tag::Empty(name, attrs) if name == "comment" => {
                parsed.comments.push(comment_style(&attrs, cursor)?);
            }
            Tag::End(name) if name == "comments" => return Ok(()),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::Eof => return Err(cursor.err("truncated comments element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

fn comment_style(
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<CommentStyle, LoadError> {
    let name = required(attrs, "name", cursor)?;
    let start = required(attrs, "start", cursor)?.to_string();
    if name == "singleLine" {
        let position = match attrs.get("position") {
            Some(value) if value.eq_ignore_ascii_case("afterwhitespace") => {
                CommentPosition::AfterWhitespace
            }
            _ => CommentPosition::Anywhere,
        };
        return Ok(CommentStyle::SingleLine { start, position });
    }
    if name == "multiLine" {
        let end = attrs.get("end").cloned().unwrap_or_default();
        return Ok(CommentStyle::MultiLine { start, end });
    }
    Err(cursor.err(format!("unknown comment kind {name}")))
}

fn parse_list(
    cursor: &mut XmlCursor,
    attrs: HashMap<String, String>,
) -> Result<ParsedList, LoadError> {
    let mut list = empty_list(&attrs, cursor)?;
    loop {
        match next_tag(cursor)? {
            Tag::Start(name, _) if name == "item" => {
                push_word(&mut list.words, &read_text(cursor, "item")?);
            }
            Tag::Empty(name, _) if name == "item" => {}
            Tag::Start(name, _) if name == "include" => {
                push_word(&mut list.includes, &read_text(cursor, "include")?);
            }
            Tag::Empty(name, _) if name == "include" => {}
            Tag::End(name) if name == "list" => return Ok(list),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::Eof => return Err(cursor.err("truncated list element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

fn empty_list(
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<ParsedList, LoadError> {
    Ok(ParsedList {
        name: required(attrs, "name", cursor)?.to_string(),
        words: Vec::new(),
        includes: Vec::new(),
    })
}

fn push_word(words: &mut Vec<String>, text: &str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        words.push(trimmed.to_string());
    }
}

fn read_text(cursor: &mut XmlCursor, name: &str) -> Result<String, LoadError> {
    let text = match cursor.reader.read_text(QName(name)) {
        Ok(text) => text,
        Err(err) => return Err(cursor.err(err.to_string())),
    };
    match quick_xml::escape::unescape(&text.into_inner()) {
        Ok(value) => Ok(value.into_owned()),
        Err(err) => Err(cursor.err(err.to_string())),
    }
}

fn parse_contexts(cursor: &mut XmlCursor, parsed: &mut ParsedDef) -> Result<(), LoadError> {
    loop {
        match next_tag(cursor)? {
            Tag::Start(name, attrs) if name == "context" => {
                parsed.contexts.push(parse_context(cursor, attrs, false)?);
            }
            Tag::Empty(name, attrs) if name == "context" => {
                parsed.contexts.push(parse_context(cursor, attrs, true)?);
            }
            Tag::End(name) if name == "contexts" => return Ok(()),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::Eof => return Err(cursor.err("truncated contexts element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

fn parse_context(
    cursor: &mut XmlCursor,
    attrs: HashMap<String, String>,
    is_empty: bool,
) -> Result<ParsedCtx, LoadError> {
    let mut ctx = ParsedCtx {
        name: attrs.get("name").cloned().unwrap_or_default(),
        attribute: required(&attrs, "attribute", cursor)?.to_string(),
        line_end: attrs.get("lineEndContext").cloned().unwrap_or_default(),
        line_empty: attrs.get("lineEmptyContext").cloned(),
        fallthrough: attrs.get("fallthroughContext").cloned(),
        no_indentation_based_folding: flag(&attrs, "noIndentationBasedFolding", cursor)?,
        stop_empty_line_context_switch_loop: flag(
            &attrs,
            "stopEmptyLineContextSwitchLoop",
            cursor,
        )?,
        items: Vec::new(),
    };
    if is_empty {
        return Ok(ctx);
    }
    loop {
        match next_tag(cursor)? {
            Tag::End(name) if name == "context" => return Ok(ctx),
            Tag::Start(name, attrs) => {
                ctx.items.push(rule_item(&name, &attrs, cursor)?);
                cursor.skip_body(&name)?;
            }
            Tag::Empty(name, attrs) => ctx.items.push(rule_item(&name, &attrs, cursor)?),
            Tag::Eof => return Err(cursor.err("truncated context element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Other => {}
        }
    }
}

fn rule_item(
    name: &str,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<PendingItem, LoadError> {
    if name == "IncludeRules" {
        return Ok(PendingItem::Include {
            context: required(attrs, "context", cursor)?.to_string(),
            include_attrib: flag(attrs, "includeAttrib", cursor)?,
        });
    }
    let kind = rule_kind(name, attrs, cursor)?;
    Ok(PendingItem::Rule(rule_from(kind, attrs, cursor)?))
}

fn rule_from(
    kind: PendingKind,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<PendingRule, LoadError> {
    let lookahead = flag(attrs, "lookAhead", cursor)?;
    let attribute = if lookahead {
        None
    } else {
        attrs.get("attribute").cloned()
    };
    let column = match attrs.get("column") {
        None => None,
        Some(value) => Some(
            value
                .parse::<u32>()
                .map_err(|_| cursor.err(format!("bad column {value}")))?,
        ),
    };
    Ok(PendingRule {
        kind,
        attribute,
        switch_raw: attrs.get("context").cloned().unwrap_or_default(),
        lookahead,
        first_non_space: flag(attrs, "firstNonSpace", cursor)?,
        column,
        begin_region: attrs.get("beginRegion").and_then(|value| non_empty(value)),
        end_region: attrs.get("endRegion").and_then(|value| non_empty(value)),
        dynamic: flag(attrs, "dynamic", cursor)?,
    })
}

fn rule_kind(
    name: &str,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<PendingKind, LoadError> {
    if let Some(kind) = kind_detect(name, attrs, cursor)? {
        return Ok(kind);
    }
    if let Some(kind) = kind_text(name, attrs, cursor)? {
        return Ok(kind);
    }
    if let Some(kind) = kind_plain(name) {
        return Ok(kind);
    }
    Err(cursor.err(format!("unknown rule {name}")))
}

fn kind_detect(
    name: &str,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<Option<PendingKind>, LoadError> {
    match name {
        "DetectChar" => Ok(Some(PendingKind::DetectChar(require_char(
            attrs, "char", cursor,
        )?))),
        "Detect2Chars" => Ok(Some(PendingKind::Detect2Chars(
            require_char(attrs, "char", cursor)?,
            require_char(attrs, "char1", cursor)?,
        ))),
        "AnyChar" => Ok(Some(PendingKind::AnyChar(require_chars(attrs, cursor)?))),
        "RangeDetect" => Ok(Some(PendingKind::RangeDetect(
            require_char(attrs, "char", cursor)?,
            require_char(attrs, "char1", cursor)?,
        ))),
        "LineContinue" => Ok(Some(PendingKind::LineContinue(line_continue_char(
            attrs, cursor,
        )?))),
        _ => Ok(None),
    }
}

fn kind_text(
    name: &str,
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<Option<PendingKind>, LoadError> {
    match name {
        "RegExpr" => Ok(Some(PendingKind::RegExpr {
            pattern: required(attrs, "String", cursor)?.to_string(),
            insensitive: flag(attrs, "insensitive", cursor)?,
            minimal: flag(attrs, "minimal", cursor)?,
        })),
        "StringDetect" => Ok(Some(PendingKind::StringDetect {
            text: required(attrs, "String", cursor)?.to_string(),
            insensitive: flag(attrs, "insensitive", cursor)?,
        })),
        "WordDetect" => Ok(Some(PendingKind::WordDetect {
            word: required(attrs, "String", cursor)?.to_string(),
            insensitive: flag(attrs, "insensitive", cursor)?,
        })),
        "keyword" => Ok(Some(PendingKind::Keyword {
            list: required(attrs, "String", cursor)?.to_string(),
            insensitive: optional_flag(attrs, "insensitive", cursor)?,
        })),
        _ => Ok(None),
    }
}

fn kind_plain(name: &str) -> Option<PendingKind> {
    Some(match name {
        "DetectIdentifier" => PendingKind::DetectIdentifier,
        "DetectSpaces" => PendingKind::DetectSpaces,
        "Float" => PendingKind::Float,
        "Int" => PendingKind::Int,
        "HlCChar" => PendingKind::HlCChar,
        "HlCHex" => PendingKind::HlCHex,
        "HlCOct" => PendingKind::HlCOct,
        "HlCStringChar" => PendingKind::HlCStringChar,
        _ => return None,
    })
}

fn parse_item_datas(cursor: &mut XmlCursor, parsed: &mut ParsedDef) -> Result<(), LoadError> {
    loop {
        match next_tag(cursor)? {
            Tag::Start(name, attrs) if name == "itemData" => {
                parsed.formats.push(parse_format(&attrs, cursor)?);
                cursor.skip_body(&name)?;
            }
            Tag::Empty(name, attrs) if name == "itemData" => {
                parsed.formats.push(parse_format(&attrs, cursor)?);
            }
            Tag::End(name) if name == "itemDatas" => return Ok(()),
            Tag::Start(name, _) => cursor.skip_body(&name)?,
            Tag::Eof => return Err(cursor.err("truncated itemDatas element")),
            Tag::End(name) => return Err(cursor.err(format!("unexpected end {name}"))),
            Tag::Empty(_, _) | Tag::Other => {}
        }
    }
}

fn parse_format(attrs: &HashMap<String, String>, cursor: &XmlCursor) -> Result<Format, LoadError> {
    let name = required(attrs, "name", cursor)?.to_string();
    let style_name = required(attrs, "defStyleNum", cursor)?;
    let style = DefaultStyle::from_name(style_name)
        .ok_or_else(|| cursor.err(format!("unknown style {style_name}")))?;
    Ok(Format {
        name,
        style,
        color: optional_rgb(attrs, "color", cursor)?,
        selected_color: optional_rgb(attrs, "selColor", cursor)?,
        background: optional_rgb(attrs, "backgroundColor", cursor)?,
        selected_background: optional_rgb(attrs, "selBackgroundColor", cursor)?,
        bold: optional_flag(attrs, "bold", cursor)?,
        italic: optional_flag(attrs, "italic", cursor)?,
        underline: optional_flag(attrs, "underline", cursor)?,
        strikeout: optional_flag(attrs, "strikeOut", cursor)?,
        spell_check: optional_flag(attrs, "spellChecking", cursor)?.unwrap_or(true),
    })
}

fn optional_rgb(
    attrs: &HashMap<String, String>,
    key: &str,
    cursor: &XmlCursor,
) -> Result<Option<Rgb>, LoadError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => parse_rgb(value)
            .map(Some)
            .ok_or_else(|| cursor.err(format!("bad color {value}"))),
    }
}

fn parse_rgb(value: &str) -> Option<Rgb> {
    let hex = value.strip_prefix('#')?;
    let bytes = hex.as_bytes();
    match bytes.len() {
        3 => Some(Rgb(
            hex_nibble(bytes[0])?,
            hex_nibble(bytes[1])?,
            hex_nibble(bytes[2])?,
        )),
        6 => Some(Rgb(
            hex_byte(&bytes[0..2])?,
            hex_byte(&bytes[2..4])?,
            hex_byte(&bytes[4..6])?,
        )),
        _ => None,
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    let value = hex_digit(byte)?;
    Some(value * 16 + value)
}

fn hex_byte(bytes: &[u8]) -> Option<u8> {
    Some(hex_digit(bytes[0])? * 16 + hex_digit(bytes[1])?)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn required<'a>(
    attrs: &'a HashMap<String, String>,
    key: &str,
    cursor: &XmlCursor,
) -> Result<&'a str, LoadError> {
    attrs
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| cursor.err(format!("missing {key}")))
}

fn flag(attrs: &HashMap<String, String>, key: &str, cursor: &XmlCursor) -> Result<bool, LoadError> {
    match attrs.get(key) {
        None => Ok(false),
        Some(value) => parse_bool(value).ok_or_else(|| cursor.err(format!("bad {key}"))),
    }
}

fn optional_flag(
    attrs: &HashMap<String, String>,
    key: &str,
    cursor: &XmlCursor,
) -> Result<Option<bool>, LoadError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(
            parse_bool(value).ok_or_else(|| cursor.err(format!("bad {key}")))?,
        )),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn require_char(
    attrs: &HashMap<String, String>,
    key: &str,
    cursor: &XmlCursor,
) -> Result<char, LoadError> {
    let value = required(attrs, key, cursor)?;
    one_char(value).ok_or_else(|| cursor.err(format!("{key} must be one character")))
}

fn require_chars(
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<Vec<char>, LoadError> {
    let value = required(attrs, "String", cursor)?;
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return Err(cursor.err("empty character set"));
    }
    Ok(chars)
}

fn line_continue_char(
    attrs: &HashMap<String, String>,
    cursor: &XmlCursor,
) -> Result<char, LoadError> {
    match attrs.get("char") {
        None => Ok('\\'),
        Some(value) => one_char(value).ok_or_else(|| cursor.err("bad line continue character")),
    }
}

fn one_char(value: &str) -> Option<char> {
    let mut chars = value.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        None
    } else {
        Some(ch)
    }
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn prepare_xml(xml: &str) -> Result<String, LoadError> {
    let Some(start) = xml.find("<!DOCTYPE") else {
        return Ok(xml.to_string());
    };
    let (subset, body) = split_doctype(xml, start)?;
    let Some(subset) = subset else {
        return Ok(body);
    };
    let entities = parse_entities(&subset)?;
    expand_custom(&body, &entities)
}

fn split_doctype(xml: &str, start: usize) -> Result<(Option<String>, String), LoadError> {
    let after = start + "<!DOCTYPE".len();
    match next_mark(xml, after)? {
        Some(Mark::Gt(at)) => Ok((None, join_without(xml, start, at + 1))),
        Some(Mark::Bracket(at)) => {
            let subset_at = at + 1;
            let end =
                find_marker(xml, subset_at, "]>")?.ok_or_else(|| plain_xml("unclosed doctype"))?;
            let subset = xml[subset_at..end].to_string();
            Ok((Some(subset), join_without(xml, start, end + 2)))
        }
        None => Err(plain_xml("unclosed doctype")),
    }
}

fn join_without(xml: &str, start: usize, end: usize) -> String {
    let mut body = String::with_capacity(xml.len() - (end - start));
    body.push_str(&xml[..start]);
    body.push_str(&xml[end..]);
    body
}

enum Mark {
    Bracket(usize),
    Gt(usize),
}

fn next_mark(xml: &str, start: usize) -> Result<Option<Mark>, LoadError> {
    let bytes = xml.as_bytes();
    let mut index = start;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        if quote.is_none() && xml[index..].starts_with("<!--") {
            index = close_comment(xml, index)?;
            continue;
        }
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'[' {
            return Ok(Some(Mark::Bracket(index)));
        }
        if byte == b'>' {
            return Ok(Some(Mark::Gt(index)));
        }
        index += 1;
    }
    Ok(None)
}

fn find_marker(xml: &str, start: usize, marker: &str) -> Result<Option<usize>, LoadError> {
    let bytes = xml.as_bytes();
    let mut index = start;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        if quote.is_none() && xml[index..].starts_with("<!--") {
            index = close_comment(xml, index)?;
            continue;
        }
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if xml[index..].starts_with(marker) {
            return Ok(Some(index));
        }
        index += 1;
    }
    Ok(None)
}

fn close_comment(xml: &str, open: usize) -> Result<usize, LoadError> {
    xml[open + 4..]
        .find("-->")
        .map(|pos| open + 4 + pos + 3)
        .ok_or_else(|| plain_xml("unclosed doctype comment"))
}

fn parse_entities(subset: &str) -> Result<HashMap<String, String>, LoadError> {
    let mut map = HashMap::new();
    let mut index = 0;
    while index < subset.len() {
        if subset[index..].starts_with("<!--") {
            index = close_comment(subset, index)?;
            continue;
        }
        if subset[index..].starts_with("<!ENTITY") {
            let (name, value, consumed) = read_entity(&subset[index..])?;
            if !name.is_empty() {
                map.insert(name, value);
            }
            index += consumed;
            continue;
        }
        index += subset[index..].chars().next().map_or(1, char::len_utf8);
    }
    Ok(map)
}

fn read_entity(input: &str) -> Result<(String, String, usize), LoadError> {
    let Some(after_prefix) = input.strip_prefix("<!ENTITY") else {
        return Err(plain_xml("bad entity"));
    };
    let trimmed = after_prefix.trim_start();
    let prefix_len = input.len() - trimmed.len();
    if trimmed.starts_with('%') {
        let end = trimmed
            .find('>')
            .ok_or_else(|| plain_xml("bad parameter entity"))?;
        return Ok((String::new(), String::new(), prefix_len + end + 1));
    }
    let name_len = trimmed
        .find(char::is_whitespace)
        .ok_or_else(|| plain_xml("bad entity name"))?;
    let name = trimmed[..name_len].to_string();
    let after_name = trimmed[name_len..].trim_start();
    let head = prefix_len + name_len + (trimmed[name_len..].len() - after_name.len());
    let mut chars = after_name.chars();
    let quote = chars
        .next()
        .ok_or_else(|| plain_xml("entity value missing"))?;
    if quote != '"' && quote != '\'' {
        return Err(plain_xml("entity value is not quoted"));
    }
    let body = &after_name[quote.len_utf8()..];
    let end = body
        .find(quote)
        .ok_or_else(|| plain_xml("unclosed entity value"))?;
    let value = body[..end].to_string();
    let consumed = head + quote.len_utf8() + end + quote.len_utf8();
    Ok((name, value, consumed))
}

fn expand_custom(input: &str, entities: &HashMap<String, String>) -> Result<String, LoadError> {
    expand_depth(input, entities, &mut Vec::new())
}

fn expand_depth(
    input: &str,
    entities: &HashMap<String, String>,
    stack: &mut Vec<String>,
) -> Result<String, LoadError> {
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        let Some(rel) = input[index..].find('&') else {
            out.push_str(&input[index..]);
            break;
        };
        let amp = index + rel;
        out.push_str(&input[index..amp]);
        index = push_entity(input, amp, entities, stack, &mut out)?;
    }
    Ok(out)
}

fn push_entity(
    input: &str,
    amp: usize,
    entities: &HashMap<String, String>,
    stack: &mut Vec<String>,
    out: &mut String,
) -> Result<usize, LoadError> {
    let Some(semi_rel) = input[amp + 1..].find(';') else {
        out.push('&');
        return Ok(amp + 1);
    };
    if semi_rel > 64 {
        out.push('&');
        return Ok(amp + 1);
    }
    let name = &input[amp + 1..amp + 1 + semi_rel];
    let next = amp + 1 + semi_rel + 1;
    if !is_custom_entity(name) {
        out.push_str(&input[amp..next]);
        return Ok(next);
    }
    let Some(value) = entities.get(name) else {
        out.push_str(&input[amp..next]);
        return Ok(next);
    };
    if stack.iter().any(|item| item == name) || stack.len() >= 32 {
        return Err(plain_xml(format!("cyclic entity {name}")));
    }
    stack.push(name.to_string());
    let expanded = expand_depth(value, entities, stack)?;
    stack.pop();
    out.push_str(&expanded);
    Ok(next)
}

fn is_custom_entity(name: &str) -> bool {
    if is_predefined(name) || !is_entity_name(name) {
        return false;
    }
    true
}

fn is_predefined(name: &str) -> bool {
    matches!(name, "lt" | "gt" | "amp" | "apos" | "quot") || name.starts_with('#')
}

fn is_entity_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' || ch == ':' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '.' | '-'))
}

fn plain_xml(message: impl Into<String>) -> LoadError {
    LoadError::Xml {
        definition: None,
        line: 0,
        message: message.into(),
    }
}
