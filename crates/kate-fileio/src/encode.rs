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
        // `wire.len()` is an upper bound on the encoded size for any
        // single-byte encoding (each Unicode scalar maps to at most one
        // byte, and each scalar takes at least one UTF-8 byte).
        let mut buf = vec![0u8; wire.len()];
        let mut encoder = opts.encoding.new_encoder();
        let (result, read, written) =
            encoder.encode_from_utf8_without_replacement(&wire, &mut buf, true);
        if let encoding_rs::EncoderResult::Unmappable(c) = result {
            let byte_offset = read - c.len_utf8();
            let (line, col, _) = position_and_char(&wire, byte_offset);
            return Err(EncodeError::Unmappable { line, col, ch: c });
        }
        buf.truncate(written);
        buf
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

/// Computes zero-based `(line, col, ch)` for the character starting at
/// byte offset `byte_offset` in `wire`.
fn position_and_char(wire: &str, byte_offset: usize) -> (u32, u32, char) {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut offset = 0usize;
    for ch in wire.chars() {
        if offset == byte_offset {
            return (line, col, ch);
        }
        let len = ch.len_utf8();
        offset += len;
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col, '\0')
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
        // 'é' (U+00E9) encodes to 0xE9 in WINDOWS-1252.
        let rope = Rope::from_str("\u{00E9}");

        let opts_ok = EncodeOptions {
            encoding: encoding_rs::WINDOWS_1252,
            bom: false,
            line_ending: LineEnding::Lf,
            ensure_trailing_newline: false,
        };
        let bytes = encode(&rope, &opts_ok).expect("WINDOWS-1252 encodes e-acute");
        assert_eq!(&bytes, &[0xE9]);

        // '€' (U+20AC) is not in ISO-8859-5 (Cyrillic), so it is unmappable.
        let rope2 = Rope::from_str("\u{20AC}");
        let opts_bad = EncodeOptions {
            encoding: encoding_rs::ISO_8859_5,
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
                ch: '\u{20AC}',
            }
        );
    }

    proptest! {
        #![proptest_config(fixed_config())]

        #[test]
        fn decode_then_encode_round_trips_bytes(
            text in prop::collection::vec(
                prop_oneof![
                    6 => prop::char::range('a', 'z'),
                    1 => Just('é'),
                    1 => Just('\n'),
                    1 => Just(' '),
                ],
                0..20,
            ).prop_map(|chars: Vec<char>| chars.into_iter().collect::<String>()),
            ending in prop_oneof![
                Just(LineEnding::Lf),
                Just(LineEnding::CrLf),
                Just(LineEnding::Cr),
            ],
        ) {
            let rope = Rope::from_str(&text);

            for encoding in [encoding_rs::UTF_8, encoding_rs::WINDOWS_1252] {
                let opts = EncodeOptions {
                    encoding,
                    bom: false,
                    line_ending: ending,
                    ensure_trailing_newline: false,
                };

                let bytes = encode(&rope, &opts)
                    .expect("encoding should succeed for safe chars");

                let dec_opts = DecodeOptions {
                    max_bytes: 10_000,
                    ..DecodeOptions::default()
                };
                let decoded = decode(&bytes, &dec_opts)
                    .expect("decoding should succeed");

                let rope2 = decoded.text;
                let bytes2 = encode(&rope2, &opts)
                    .expect("re-encoding should succeed");

                assert_eq!(&bytes, &bytes2);
            }
        }
    }
}
