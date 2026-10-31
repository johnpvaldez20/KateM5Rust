// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: MIT

//! Golden spans from upstream autotests/reference.

use std::fs;
use std::path::PathBuf;

use kate_syntax::{Highlighter, LineHighlight, Repository};

const VENDORED: &[&str] = &[
    include_str!("../definitions/rust.xml"),
    include_str!("../definitions/comments.xml"),
    include_str!("../definitions/alert.xml"),
    include_str!("../definitions/modelines.xml"),
    include_str!("../definitions/spdx-comments.xml"),
    include_str!("../definitions/ansic89.xml"),
];

#[test]
fn upstream_reference_output_matches() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/golden");
    let mut cases = fs::read_dir(&root)
        .expect("golden dir")
        .flatten()
        .collect::<Vec<_>>();
    cases.sort_by_key(|entry| entry.file_name());
    assert!(!cases.is_empty(), "no golden cases");
    for case in cases {
        if !case.path().is_dir() {
            continue;
        }
        let name = fs::read_to_string(case.path().join("definition")).expect("definition");
        let name = name.trim();
        let owned = fs::read_to_string(case.path().join("input")).expect("input");
        // QTextStream drops a leading BOM. The reference lines do not contain it.
        let input = owned.trim_start_matches('\u{feff}');
        let expected = fs::read_to_string(case.path().join("expected.spans")).expect("expected");
        let repo = load_vendored();
        let id = repo
            .by_name(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let mut hl = Highlighter::new(&repo, id);
        let got = render(&repo, &mut hl, input);
        assert_spans_equal(name, input, &got, &expected);
    }
}

fn load_vendored() -> Repository {
    let mut repo = Repository::empty();
    for xml in VENDORED {
        repo.add_xml(xml).expect("vendored file parses");
    }
    repo.link().expect("vendored files link");
    repo
}

fn render(repo: &Repository, hl: &mut Highlighter<'_>, text: &str) -> String {
    let mut out = LineHighlight::default();
    let mut state = hl.initial_state();
    let mut rendered = String::new();
    for line in source_lines(text) {
        hl.highlight_line(line, state, &mut out);
        state = out.end_state;
        for span in &out.spans {
            let format = &repo.format(span.format).name;
            rendered.push_str(&format!("{}\t{}\t{}\n", span.start, span.len, format));
        }
        rendered.push('\n');
    }
    rendered
}

fn assert_spans_equal(name: &str, input: &str, got: &str, expected: &str) {
    if got == expected {
        return;
    }
    let lines = source_lines(input);
    let got_blocks: Vec<&str> = got.split("\n\n").collect();
    let expected_blocks: Vec<&str> = expected.split("\n\n").collect();
    for (index, line) in lines.iter().enumerate() {
        let left = got_blocks.get(index).copied().unwrap_or("<missing>");
        let right = expected_blocks.get(index).copied().unwrap_or("<missing>");
        if left != right {
            panic!("{name} line {index} {line:?}\nexpected:\n{right}\ngot:\n{left}");
        }
    }
    panic!("{name} span bytes differ after the last source line");
}

fn source_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    if rest.is_empty() {
        return lines;
    }
    loop {
        if let Some((line, tail)) = rest.split_once('\n') {
            lines.push(line.strip_suffix('\r').unwrap_or(line));
            rest = tail;
            if rest.is_empty() {
                break;
            }
        } else {
            lines.push(rest.strip_suffix('\r').unwrap_or(rest));
            break;
        }
    }
    lines
}
