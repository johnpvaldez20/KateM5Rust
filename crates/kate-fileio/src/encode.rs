// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Encodes a rope to bytes with a chosen encoding, BOM, and line ending.
//!
//! Converts `\n` to the target line ending. Prepend a BOM when requested.
//! A character that the target encoding cannot represent is an error
//! naming line and column, never a silent `?`.

use crate::{Encoding, LineEnding};
use ropey::Rope;

/// Options controlling how text is encoded to bytes.
#[derive(Clone, Debug)]
pub struct EncodeOptions {
    /// Target character encoding.
    pub encoding: Encoding,
    /// Prepend the encoding's BOM when `true`.
    pub bom: bool,
    /// Line ending to write in place of `\n`.
    pub line_ending: LineEnding,
    /// Append a line ending when the text does not already end with one.
    pub ensure_trailing_newline: bool,
}

/// A character in the text cannot be represented in the target encoding.
#[derive(Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// `line` and `col` are zero-based.
    Unmappable { line: u32, col: u32, ch: char },
}

/// Converts `\n` to the requested ending, optionally prepends a BOM,
/// and encodes. Fails on the first unmappable character.
pub fn encode(text: &Rope, opts: &EncodeOptions) -> Result<Vec<u8>, EncodeError> {
    let s = text.to_string();
    let wire = to_wire_form(&s, opts);

    // Encode the body. `encode_from_utf8_without_replacement` takes
    // `&mut [u8]`, so we must pre-size the buffer; a `Vec::new()` would
    // coerce to a zero-length slice and the encoder could write nothing.
    let body: Vec<u8> = if opts.encoding == encoding_rs::UTF_8 {
        wire.as_bytes().to_vec()
    } else if opts.encoding == encoding_rs::UTF_16LE {
        let mut v = Vec::with_capacity(wire.encode_utf16().count() * 2);
        encode_utf16_bytes(&wire, &mut v, true);
        v
    } else if opts.encoding == encoding_rs::UTF_16BE {
        let mut v = Vec::with_capacity(wire.encode_utf16().count() * 2);
        encode_utf16_bytes(&wire, &mut v, false);
        v
    } else {
        let mut encoder = opts.encoding.new_encoder();
        let mut total_read = 0usize;
        let mut total_written = 0usize;
        let mut buf = vec![0u8; wire.len() * 2 + 16];
        loop {
            let (result, read, written) = encoder.encode_from_utf8_without_replacement(
                &wire[total_read..],
                &mut buf[total_written..],
                false,
            );
            total_read += read;
            total_written += written;
            match result {
                encoding_rs::EncoderResult::InputEmpty => {
                    // Flush any pending state.
                    buf.resize(total_written + 4, 0);
                    let (_, _, written) =
                        encoder.encode_from_utf8_without_replacement("", &mut buf[total_written..], true);
                    total_written += written;
                    buf.truncate(total_written);
                    break buf;
                }
                encoding_rs::EncoderResult::Unmappable(c) => {
                    let wire_byte_offset = total_read - c.len_utf8();
                    let wire_char_idx = wire[..wire_byte_offset].chars().count();
                    let src_char_idx = wire_char_to_src_char(wire_char_idx, &s, &opts.line_ending);
                    let (line, col) = line_col_at(&s, src_char_idx);
                    return Err(EncodeError::Unmappable { line, col, ch: c });
                }
                encoding_rs::EncoderResult::OutputFull => {
                    buf.resize(buf.len() * 2 + 16, 0);
                }
            }
        }
    };

    let mut out = Vec::with_capacity(body.len() + 3);
    write_bom(&mut out, opts);
    out.extend_from_slice(&body);
    Ok(out)
}

fn write_bom(out: &mut Vec<u8>, opts: &EncodeOptions) {
    if !opts.bom {
        return;
    }
    match opts.encoding {
        e if e == encoding_rs::UTF_8 => out.extend_from_slice(&[0xEF, 0xBB, 0xBF]),
        e if e == encoding_rs::UTF_16LE => out.extend_from_slice(&[0xFF, 0xFE]),
        e if e == encoding_rs::UTF_16BE => out.extend_from_slice(&[0xFE, 0xFF]),
        _ => {}
    }
}

fn encode_utf16_bytes(s: &str, out: &mut Vec<u8>, little_endian: bool) {
    for unit in s.encode_utf16() {
        if little_endian {
            out.extend_from_slice(&unit.to_le_bytes());
        } else {
            out.extend_from_slice(&unit.to_be_bytes());
        }
    }
}

/// Converts `\n` to the target line ending and optionally ensures a trailing newline.
fn to_wire_form(s: &str, opts: &EncodeOptions) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\n' {
            out.push_str(opts.line_ending.as_str());
        } else {
            out.push(ch);
        }
    }
    if opts.ensure_trailing_newline && !s.is_empty() && !s.ends_with('\n') {
        out.push_str(opts.line_ending.as_str());
    }
    out
}

/// Computes zero-based `(line, col)` for the character at char index `idx`
/// in `s` (which uses `\n` line endings).
fn line_col_at(s: &str, idx: usize) -> (u32, u32) {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    for (i, ch) in s.chars().enumerate() {
        if i == idx {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Maps a character index in the line-ending-converted `wire` text back to
/// a character index in the original `s` text (which uses `\n`).
fn wire_char_to_src_char(wire_idx: usize, s: &str, line_ending: &LineEnding) -> usize {
    let end_chars = line_ending.as_str().chars().count();
    let mut wire_pos = 0usize;
    let mut src_idx = 0usize;
    for ch in s.chars() {
        if wire_pos >= wire_idx {
            return src_idx;
        }
        wire_pos += if ch == '\n' { end_chars } else { 1 };
        src_idx += 1;
    }
    src_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, DecodeOptions};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    /// Fixed seed so a local run and a CI run use the same cases.
    const PROPTEST_SEED: u64 = 0x504B_5400_0000_0006;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    #[test]
    fn unmappable_char_is_an_error_with_position() {
        // encoding_rs has no ISO-8859-1 (the label maps to windows-1252,
        // where '€' is 0x80), so we test with ISO_8859_15.

        // '€' (U+20AC) encodes to 0xA4 in ISO_8859_15.
        let rope = Rope::from_str("\u{20AC}");
        let opts_ok = EncodeOptions {
            encoding: encoding_rs::ISO_8859_15,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts_ok).expect("ISO_8859_15 encodes euro sign");
        assert_eq!(&bytes, &[0xA4]);

        // '¤' (U+00A4) is not in ISO_8859_15, so it is unmappable.
        let rope2 = Rope::from_str("\u{00A4}");
        let opts_bad = EncodeOptions {
            encoding: encoding_rs::ISO_8859_15,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let err = encode(&rope2, &opts_bad).unwrap_err();
        assert_eq!(
            err,
            EncodeError::Unmappable {
                line: 0,
                col: 0,
                ch: '\u{00A4}',
            }
        );
    }

    #[test]
    fn gb18030_encodes_multi_byte() {
        // 'é' (U+00E9) encodes to [0xA8, 0xA6] in GB18030.
        let rope = Rope::from_str("\u{00E9}");
        let opts = EncodeOptions {
            encoding: encoding_rs::GB18030,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts).expect("GB18030 encodes e-acute");
        assert_eq!(&bytes, &[0xA8, 0xA6]);
    }

    #[test]
    fn iso2022jp_encodes_via_encoding_rs() {
        let text = "aあ";
        let rope = Rope::from_str(text);
        let opts = EncodeOptions {
            encoding: encoding_rs::ISO_2022_JP,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts).expect("ISO_2022_JP encodes aあ");
        let (expected, _, _) = encoding_rs::ISO_2022_JP.encode(text);
        assert_eq!(&bytes[..], &expected[..]);
    }

    #[test]
    fn iso2022jp_long_alternating_matches_encoding_rs() {
        let text = "aあ".repeat(200);
        let rope = Rope::from_str(&text);
        let opts = EncodeOptions {
            encoding: encoding_rs::ISO_2022_JP,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts).expect("ISO_2022_JP encodes long alternating text");
        let (expected, _, _) = encoding_rs::ISO_2022_JP.encode(&text);
        assert_eq!(&bytes[..], &expected[..]);
    }

    #[test]
    fn gb18030_long_repeated_matches_encoding_rs() {
        let text = "é".repeat(200);
        let rope = Rope::from_str(&text);
        let opts = EncodeOptions {
            encoding: encoding_rs::GB18030,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts).expect("GB18030 encodes long repeated text");
        let (expected, _, _) = encoding_rs::GB18030.encode(&text);
        assert_eq!(&bytes[..], &expected[..]);
    }

    #[test]
    fn unmappable_on_second_line_reports_correct_position() {
        // '€' (U+20AC) is not in ISO_8859_2 (Latin-2), so it is unmappable.
        // Line/col are computed on the original \n text, so the result is
        // the same regardless of the output line ending.
        let text = "ab\ncd\u{20AC}";
        for &ending in &[LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr] {
            let rope = Rope::from_str(text);
            let opts = EncodeOptions {
                encoding: encoding_rs::ISO_8859_2,
                bom: false,
                line_ending: ending,
                ensure_trailing_newline: false,
            };
            let err = encode(&rope, &opts).unwrap_err();
            assert_eq!(
                err,
                EncodeError::Unmappable {
                    line: 1,
                    col: 2,
                    ch: '\u{20AC}',
                },
                "wrong position for ending {ending:?}"
            );
        }
    }

    proptest! {
        #![proptest_config(fixed_config())]

        #[test]
        fn decode_then_encode_round_trips_bytes(
            encoding in prop_oneof![
                Just(encoding_rs::UTF_8),
                Just(encoding_rs::WINDOWS_1252),
            ],
            ending in prop_oneof![
                Just(LineEnding::Lf),
                Just(LineEnding::CrLf),
                Just(LineEnding::Cr),
            ],
            bom in any::<bool>(),
            lines in prop::collection::vec(
                prop::collection::vec(
                    prop_oneof![
                        6 => prop::char::range('a', 'z'),
                        1 => Just(' '),
                        1 => Just('é'),
                    ],
                    0..8,
                ).prop_map(|chars: Vec<char>| chars.into_iter().collect::<String>()),
                0..5,
            ),
        ) {
            // Build expected on-disk bytes independently of encode().
            let ending_bytes: &[u8] = match ending {
                LineEnding::Lf => &[0x0A],
                LineEnding::CrLf => &[0x0D, 0x0A],
                LineEnding::Cr => &[0x0D],
            };

            let mut bytes: Vec<u8> = Vec::new();

            // BOM only for UTF_8.
            if bom && encoding == encoding_rs::UTF_8 {
                bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }

            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    bytes.extend_from_slice(ending_bytes);
                }
                if encoding == encoding_rs::UTF_8 {
                    bytes.extend_from_slice(line.as_bytes());
                } else {
                    // WINDOWS_1252: 'é' → 0xE9, a-z and space are ASCII.
                    for ch in line.chars() {
                        if ch == 'é' {
                            bytes.push(0xE9);
                        } else {
                            bytes.push(ch as u8);
                        }
                    }
                }
            }

            let dec_opts = DecodeOptions {
                max_bytes: 10_000,
                fallback: encoding,
            };
            let decoded = decode(&bytes, &dec_opts)
                .expect("decoding should succeed");

            // Re-encode using metadata from the decode.
            let re_opts = EncodeOptions {
                encoding: decoded.encoding,
                bom: decoded.bom,
                line_ending: decoded.line_ending,
                ensure_trailing_newline: false,
            };
            let bytes2 = encode(&decoded.text, &re_opts)
                .expect("re-encoding should succeed");

            assert_eq!(&bytes, &bytes2);
        }
    }
}
