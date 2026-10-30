// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Byte decoding and line-ending normalization.
//!
//! Detection order is a BOM, then strict UTF-8, then the fallback encoding.
//! The rope stores LF only. The majority ending is remembered.

use ropey::Rope;

/// The line ending stored for a later save.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    /// Returns the bytes for this ending.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

/// Alias so callers never spell the encoding_rs path.
pub type Encoding = &'static encoding_rs::Encoding;

/// Limits and the encoding used when the bytes are not UTF-8 and have no BOM.
#[derive(Clone, Debug)]
pub struct DecodeOptions {
    /// Used when the bytes are not valid UTF-8 and carry no BOM.
    pub fallback: Encoding,
    /// Refuse a larger input instead of reading it.
    pub max_bytes: u64,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            fallback: encoding_rs::WINDOWS_1252,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// Text plus the facts `decode` learned from the bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    /// Line endings are normalized to `\n`.
    pub text: Rope,
    pub encoding: Encoding,
    pub bom: bool,
    /// Majority ending in the input. `Lf` when the input has no ending.
    pub line_ending: LineEnding,
    /// True when a replacement character was inserted.
    pub had_malformed: bool,
    /// True when a NUL is in the first 8 KiB.
    /// UTF-16 with a BOM checks the decoded text for U+0000.
    pub probably_binary: bool,
}

/// `decode` rejected the input before producing text.
#[derive(Debug)]
pub enum DecodeError {
    TooLarge { size: u64, limit: u64 },
}

/// Two gibibytes. A larger file is refused before it is decoded.
const DEFAULT_MAX_BYTES: u64 = 1 << 31;

/// A NUL in this prefix marks the buffer as probably binary.
const BINARY_PREFIX: usize = 8 * 1024;

/// Decodes `bytes` under `opts`.
pub fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded, DecodeError> {
    let size = u64::try_from(bytes.len()).expect("byte length fits in u64");
    if size > opts.max_bytes {
        return Err(DecodeError::TooLarge {
            size,
            limit: opts.max_bytes,
        });
    }
    let (bom_encoding, bom, payload) = match encoding_rs::Encoding::for_bom(bytes) {
        Some((encoding, bom_len)) => (Some(encoding), true, &bytes[bom_len..]),
        None => (None, false, bytes),
    };
    let (text, encoding, had_malformed) = decode_payload(payload, bom_encoding, opts.fallback);
    let (normalized, line_ending) = normalize_endings(&text);
    Ok(Decoded {
        text: Rope::from_str(&normalized),
        encoding,
        bom,
        line_ending,
        had_malformed,
        probably_binary: probably_binary(bytes, bom, encoding, &text),
    })
}

fn probably_binary(bytes: &[u8], bom: bool, encoding: Encoding, text: &str) -> bool {
    if bom && is_utf16(encoding) {
        return decoded_prefix_has_nul(text);
    }
    bytes.iter().take(BINARY_PREFIX).any(|byte| *byte == 0)
}

fn is_utf16(encoding: Encoding) -> bool {
    encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE
}

/// `true` when U+0000 starts inside the first 8 KiB of decoded text.
fn decoded_prefix_has_nul(text: &str) -> bool {
    let mut seen = 0usize;
    for ch in text.chars() {
        if seen >= BINARY_PREFIX {
            return false;
        }
        if ch == '\u{0000}' {
            return true;
        }
        seen += ch.len_utf8();
    }
    false
}

fn decode_payload(
    payload: &[u8],
    bom_encoding: Option<Encoding>,
    fallback: Encoding,
) -> (String, Encoding, bool) {
    if let Some(encoding) = bom_encoding {
        let (text, had_errors) = encoding.decode_without_bom_handling(payload);
        return (text.into_owned(), encoding, had_errors);
    }
    if let Ok(text) = std::str::from_utf8(payload) {
        return (text.to_owned(), encoding_rs::UTF_8, false);
    }
    let (text, had_errors) = fallback.decode_without_bom_handling(payload);
    (text.into_owned(), fallback, had_errors)
}

fn normalize_endings(text: &str) -> (String, LineEnding) {
    let mut out = String::with_capacity(text.len());
    let mut counts = [0usize; 3];
    let mut first = [None; 3];
    let mut seen = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        let kind = if ch == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
            1
        } else if ch == '\r' {
            2
        } else if ch == '\n' {
            0
        } else {
            out.push(ch);
            continue;
        };
        out.push('\n');
        if first[kind].is_none() {
            first[kind] = Some(seen);
        }
        counts[kind] += 1;
        seen += 1;
    }
    (out, majority(counts, first))
}

fn majority(counts: [usize; 3], first: [Option<usize>; 3]) -> LineEnding {
    let max = counts.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return LineEnding::Lf;
    }
    let kinds = [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr];
    let mut best = LineEnding::Lf;
    let mut best_at = usize::MAX;
    for (index, kind) in kinds.into_iter().enumerate() {
        if counts[index] != max {
            continue;
        }
        let at = first[index].unwrap_or(usize::MAX);
        if at < best_at {
            best = kind;
            best_at = at;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, DecodeOptions, LineEnding, decode};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    /// Fixed seed so a local run and a CI run use the same cases.
    const PROPTEST_SEED: u64 = 0x504B_5400_0000_0005;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    #[test]
    fn utf8_bom_is_stripped_and_remembered() {
        let decoded = decode(b"\xEF\xBB\xBFhi", &DecodeOptions::default()).unwrap();
        assert_eq!(decoded.text.to_string(), "hi");
        assert!(decoded.bom);
        assert_eq!(decoded.encoding, encoding_rs::UTF_8);
        assert!(!decoded.had_malformed);
    }

    #[test]
    fn utf16le_bom_decodes() {
        let bytes = utf16le_with_bom("héllo");
        let decoded = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert_eq!(decoded.text.to_string(), "héllo");
        assert_eq!(decoded.encoding, encoding_rs::UTF_16LE);
        assert!(decoded.bom);
        assert!(!decoded.had_malformed);
        assert!(!decoded.probably_binary);
    }

    #[test]
    fn bomless_utf16le_is_binary() {
        let mut bytes = Vec::new();
        for unit in "héllo".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let decoded = decode(&bytes, &DecodeOptions::default()).unwrap();
        assert!(!decoded.bom);
        assert!(decoded.probably_binary);
        assert_eq!(decoded.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn utf16le_nul_scalar_in_the_prefix_is_binary() {
        let decoded = decode(&utf16le_with_bom("a\0b"), &DecodeOptions::default()).unwrap();
        assert!(decoded.bom);
        assert!(decoded.probably_binary);
        assert_eq!(decoded.text.to_string(), "a\0b");
    }

    #[test]
    fn invalid_utf8_falls_back_to_configured_encoding() {
        let decoded = decode(&[0xE9], &DecodeOptions::default()).unwrap();
        assert_eq!(decoded.text.to_string(), "é");
        assert!(!decoded.had_malformed);
        assert_eq!(decoded.encoding, encoding_rs::WINDOWS_1252);
        assert!(!decoded.bom);
    }

    #[test]
    fn line_ending_as_str_matches_the_ending() {
        assert_eq!(LineEnding::Lf.as_str(), "\n");
        assert_eq!(LineEnding::CrLf.as_str(), "\r\n");
        assert_eq!(LineEnding::Cr.as_str(), "\r");
    }

    #[test]
    fn over_max_bytes_is_rejected() {
        let opts = DecodeOptions {
            max_bytes: 4,
            ..DecodeOptions::default()
        };
        assert!(matches!(
            decode(b"hello", &opts),
            Err(DecodeError::TooLarge { size: 5, limit: 4 })
        ));
        let at_limit = DecodeOptions {
            max_bytes: 5,
            ..DecodeOptions::default()
        };
        assert!(decode(b"hello", &at_limit).is_ok());
    }

    #[test]
    fn adjacent_cr_and_lf_are_one_crlf() {
        // The last CR is followed by LF, so those two bytes are one CR LF.
        let decoded = decode(b"\n\r\r\r\n", &DecodeOptions::default()).unwrap();
        assert_eq!(decoded.text.to_string(), "\n\n\n\n");
        assert_eq!(decoded.line_ending, LineEnding::Cr);
        assert_eq!(decoded.text.len_lines(), 5);
    }

    #[test]
    fn tied_endings_keep_the_earliest() {
        let lf_first = decode(b"\na\r", &DecodeOptions::default()).unwrap();
        assert_eq!(lf_first.text.to_string(), "\na\n");
        assert_eq!(lf_first.line_ending, LineEnding::Lf);

        let cr_first = decode(b"\ra\n", &DecodeOptions::default()).unwrap();
        assert_eq!(cr_first.text.to_string(), "\na\n");
        assert_eq!(cr_first.line_ending, LineEnding::Cr);
    }

    #[test]
    fn utf8_bom_keeps_utf8_when_the_tail_is_invalid() {
        let decoded = decode(b"\xEF\xBB\xBF\xFF", &DecodeOptions::default()).unwrap();
        assert!(decoded.bom);
        assert_eq!(decoded.encoding, encoding_rs::UTF_8);
        assert!(decoded.had_malformed);
        assert!(decoded.text.to_string().contains('\u{FFFD}'));
    }

    #[test]
    fn nul_in_the_first_8kib_marks_binary() {
        let opts = DecodeOptions::default();
        let clean = decode(b"hello", &opts).unwrap();
        assert!(!clean.probably_binary);

        let mut early = vec![b'a'; 8 * 1024];
        early[8 * 1024 - 1] = 0;
        assert!(decode(&early, &opts).unwrap().probably_binary);

        let mut late = vec![b'a'; 8 * 1024];
        late.push(0);
        assert!(!decode(&late, &opts).unwrap().probably_binary);
    }

    fn utf16le_with_bom(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    #[derive(Clone, Debug)]
    struct Sample {
        bytes: Vec<u8>,
        normalized: String,
        majority: LineEnding,
        logical_lines: u32,
    }

    fn arb_ending() -> impl Strategy<Value = LineEnding> {
        prop_oneof![
            Just(LineEnding::Lf),
            Just(LineEnding::CrLf),
            Just(LineEnding::Cr),
        ]
    }

    fn arb_line() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                8 => prop::char::range('a', 'z'),
                1 => Just('é'),
                1 => Just('你'),
                1 => Just('\u{1F600}'),
            ],
            1..8,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    fn push_ending(raw: &mut String, ending: LineEnding) {
        match ending {
            LineEnding::Lf => raw.push('\n'),
            LineEnding::CrLf => raw.push_str("\r\n"),
            LineEnding::Cr => raw.push('\r'),
        }
    }

    fn other_endings(winner: LineEnding) -> (LineEnding, LineEnding) {
        match winner {
            LineEnding::Lf => (LineEnding::CrLf, LineEnding::Cr),
            LineEnding::CrLf => (LineEnding::Lf, LineEnding::Cr),
            LineEnding::Cr => (LineEnding::Lf, LineEnding::CrLf),
        }
    }

    fn assemble(lines: &[String], endings: &[LineEnding], majority: LineEnding) -> Sample {
        let mut raw = String::new();
        let mut normalized = String::new();
        for (index, ending) in endings.iter().enumerate() {
            let line = &lines[index % lines.len()];
            raw.push_str(line);
            normalized.push_str(line);
            push_ending(&mut raw, *ending);
            normalized.push('\n');
        }
        Sample {
            bytes: raw.into_bytes(),
            normalized,
            majority,
            logical_lines: u32::try_from(endings.len() + 1).unwrap(),
        }
    }

    fn arb_sample() -> impl Strategy<Value = Sample> {
        (
            arb_ending(),
            1usize..5,
            arb_line(),
            prop::collection::vec(arb_line(), 0..4),
        )
            .prop_flat_map(|(winner, win_count, first_line, more_lines)| {
                (
                    Just(winner),
                    Just(win_count),
                    Just(first_line),
                    Just(more_lines),
                    0..win_count,
                    0..win_count,
                )
            })
            .prop_map(
                |(winner, win_count, first_line, more_lines, left_count, right_count)| {
                    let (left, right) = other_endings(winner);
                    let mut endings = Vec::new();
                    endings.extend(std::iter::repeat_n(left, left_count.div_ceil(2)));
                    endings.extend(std::iter::repeat_n(winner, win_count));
                    endings.extend(std::iter::repeat_n(left, left_count / 2));
                    endings.extend(std::iter::repeat_n(right, right_count));
                    let mut lines = vec![first_line];
                    lines.extend(more_lines);
                    assemble(&lines, &endings, winner)
                },
            )
    }

    fn fixed_cr_between_line_feeds() -> Sample {
        assemble(
            &[
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
            ],
            &[
                LineEnding::Lf,
                LineEnding::Cr,
                LineEnding::Cr,
                LineEnding::Cr,
                LineEnding::Lf,
            ],
            LineEnding::Cr,
        )
    }

    fn mixed_samples() -> impl Strategy<Value = Sample> {
        prop_oneof![
            8 => arb_sample(),
            1 => Just(fixed_cr_between_line_feeds()),
            1 => Just(assemble(
                &["a".to_string(), "b".to_string(), "c".to_string()],
                &[LineEnding::CrLf, LineEnding::CrLf, LineEnding::Lf],
                LineEnding::CrLf,
            )),
        ]
    }

    proptest! {
        #![proptest_config(fixed_config())]
        #[test]
        fn crlf_normalized_and_majority_ending_recorded(sample in mixed_samples()) {
            let decoded = decode(&sample.bytes, &DecodeOptions::default()).unwrap();
            let text = decoded.text.to_string();
            prop_assert!(text.chars().all(|ch| ch != '\r'));
            prop_assert_eq!(text, sample.normalized);
            prop_assert_eq!(
                u32::try_from(decoded.text.len_lines()).unwrap(),
                sample.logical_lines
            );
            prop_assert_eq!(decoded.line_ending, sample.majority);
        }
    }
}
