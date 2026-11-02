// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! A generated Rust file for the scroll benchmark.

use std::io::{self, Write};
use std::path::Path;

/// Line count of the scroll benchmark file.
pub const FIXTURE_LINE_COUNT: u32 = 100_000;

/// Write `count` Rust lines. The file has no extra blank line at the end.
pub(crate) fn write_rust_lines(out: &mut impl Write, count: u32) -> io::Result<()> {
    for index in 0..count {
        if index > 0 {
            out.write_all(b"\n")?;
        }
        if index == 1 {
            write!(out, "// 日本語 emoji 😀 font fallback")?;
        } else {
            write!(
                out,
                "pub fn item_{index:06}() -> i32 {{ let name = \"item_{index:06}\"; if name.len() > 0 {{ return 1; }} 0 }}"
            )?;
        }
    }
    Ok(())
}

/// Write `count` lines through one buffered writer.
///
/// `File::write` of each small line is one syscall. `BufWriter` groups them.
pub(crate) fn write_buffered(out: impl Write, count: u32) -> io::Result<()> {
    let mut out = std::io::BufWriter::new(out);
    write_rust_lines(&mut out, count)?;
    out.flush()
}

/// Write a 100,000-line Rust file to `path`.
pub fn write_rust_fixture(path: &Path) -> io::Result<()> {
    let file = std::fs::File::create(path)?;
    write_buffered(file, FIXTURE_LINE_COUNT)
}

#[cfg(test)]
mod tests {
    use super::{FIXTURE_LINE_COUNT, write_buffered, write_rust_lines};
    use std::io::{self, Write};

    #[test]
    fn generator_writes_100_000_rust_lines() {
        let mut buf = Vec::new();
        write_rust_lines(&mut buf, FIXTURE_LINE_COUNT).expect("write");
        let text = String::from_utf8(buf).expect("utf-8");
        assert_eq!(text.lines().count(), 100_000);
        assert!(!text.ends_with('\n'));
        assert!(text.contains("fn item_"));
        assert!(text.contains('😀'));
        assert!(text.contains("日本語"));
    }

    #[test]
    fn fixture_writer_buffers_many_small_line_writes() {
        let mut counter = WriteCount::default();
        let lines = 2_000u32;
        write_buffered(&mut counter, lines).expect("write");
        assert!(
            counter.writes < lines as usize,
            "writes {} for {lines} lines",
            counter.writes
        );
        let direct = {
            let mut buf = Vec::new();
            write_rust_lines(&mut buf, lines).expect("write");
            buf
        };
        assert_eq!(counter.bytes, direct);
    }

    #[derive(Default)]
    struct WriteCount {
        writes: usize,
        bytes: Vec<u8>,
    }

    impl Write for WriteCount {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
