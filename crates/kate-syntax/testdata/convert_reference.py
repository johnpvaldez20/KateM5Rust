#!/usr/bin/env python3
# Convert one KSyntaxHighlighting autotests reference file to span lines.
#
# The upstream test writes one line as format tags plus "<br/>".
# This script turns each tag into "start<TAB>len<TAB>format name".
# A blank line follows each source line.
#
# Usage:
#   python3 convert_reference.py REFERENCE INPUT EXPECTED
#
# REFERENCE is autotests/reference/<file>.ref from syntax-highlighting
# commit 2d07614ca2ba8565915cc7e0959fd44745702ca1.
# INPUT is the matching autotests/input file.
# The script checks that the tag text equals the input line.
# It writes EXPECTED. Commit that file. Do not commit upstream/.

import pathlib
import sys


def parse_line(body: str) -> list[tuple[str, str]]:
    spans: list[tuple[str, str]] = []
    index = 0
    while index < len(body):
        if not body.startswith("<", index):
            raise SystemExit(f"expected a tag at {index}: {body[index:index + 40]!r}")
        end = body.find(">", index)
        if end < 0:
            raise SystemExit("truncated tag")
        name = body[index + 1 : end]
        if name == "br/":
            break
        close = f"</{name}>"
        found = body.find(close, end + 1)
        if found < 0:
            raise SystemExit(f"missing close tag {close}")
        spans.append((name, body[end + 1 : found]))
        index = found + len(close)
    return spans


def source_lines(text: str) -> list[str]:
    if text == "":
        return []
    lines: list[str] = []
    rest = text
    while True:
        if "\n" not in rest:
            lines.append(rest[:-1] if rest.endswith("\r") else rest)
            break
        line, rest = rest.split("\n", 1)
        if line.endswith("\r"):
            line = line[:-1]
        lines.append(line)
        if rest == "":
            break
    return lines


def convert(reference: str, source: str) -> str:
    ref_lines = reference.splitlines()
    src_lines = source_lines(source)
    if len(ref_lines) != len(src_lines):
        raise SystemExit(f"line count {len(ref_lines)} != {len(src_lines)}")
    parts: list[str] = []
    for ref, src in zip(ref_lines, src_lines):
        if not ref.endswith("<br/>"):
            raise SystemExit(f"reference line has no <br/>: {ref!r}")
        spans = parse_line(ref[: -len("<br/>")])
        text = "".join(piece for _, piece in spans)
        if text != src:
            raise SystemExit(f"tag text does not match input:\n{text!r}\n{src!r}")
        column = 0
        for name, piece in spans:
            length = len(piece)
            parts.append(f"{column}\t{length}\t{name}\n")
            column += length
        parts.append("\n")
    return "".join(parts)


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: convert_reference.py REFERENCE INPUT EXPECTED")
    reference = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
    source = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
    # QTextStream drops a leading BOM before readLine. The reference has no BOM.
    source = source.removeprefix("\ufeff")
    expected = convert(reference, source)
    pathlib.Path(sys.argv[3]).write_text(expected, encoding="utf-8")


if __name__ == "__main__":
    main()
