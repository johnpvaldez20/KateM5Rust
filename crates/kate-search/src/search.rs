// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: LGPL-2.1-or-later

//! Regression tests for packet PKT-0010.

#[cfg(test)]
mod tests {
    use crate::{
        Match, Pattern, SearchError, SearchQuery, expand_replacement, find_all, find_next,
        replace_all,
    };
    use kate_text::{CharRange, Offset, TextBuffer};
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    /// Fixed seed so a local run and a CI run use the same cases.
    const PROPTEST_SEED: u64 = 0x504B_5400_0000_0010;

    fn fixed_config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn query(pattern: Pattern, case_sensitive: bool, whole_words: bool) -> SearchQuery {
        SearchQuery {
            pattern,
            case_sensitive,
            whole_words,
            range: None,
        }
    }

    fn plain_ranges(text: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
        let buf = TextBuffer::from_str(text);
        let found = find_all(
            &buf,
            &query(Pattern::Plain(needle.to_string()), case_sensitive, false),
        )
        .expect("plain search does not fail");
        found
            .into_iter()
            .map(|hit| {
                assert!(hit.captures.is_empty());
                (hit.range.start().0, hit.range.end().0)
            })
            .collect()
    }

    fn char_spans<'a>(
        hay: &str,
        matches: impl Iterator<Item = (usize, &'a str)>,
    ) -> Vec<(usize, usize)> {
        matches
            .map(|(byte, text)| {
                let start = hay[..byte].chars().count();
                let end = start + text.chars().count();
                (start, end)
            })
            .collect()
    }

    fn fold_origins(text: &str) -> Vec<usize> {
        let mut map = Vec::new();
        for (index, ch) in text.chars().enumerate() {
            let count = if ch == 'Σ' {
                1
            } else {
                ch.to_lowercase().count()
            };
            for _ in 0..count {
                map.push(index);
            }
        }
        map
    }

    fn map_folded_span(
        map: &[usize],
        orig_len: usize,
        start_f: usize,
        end_f: usize,
    ) -> (usize, usize) {
        if start_f == end_f {
            let pos = if start_f >= map.len() {
                orig_len
            } else {
                map[start_f]
            };
            return (pos, pos);
        }
        (map[start_f], map[end_f - 1] + 1)
    }

    fn oracle(text: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
        if case_sensitive {
            return char_spans(text, text.match_indices(needle));
        }
        let folded = text.to_lowercase();
        let needle_folded = needle.to_lowercase();
        let map = fold_origins(text);
        assert_eq!(
            map.len(),
            folded.chars().count(),
            "fold map does not match to_lowercase"
        );
        char_spans(&folded, folded.match_indices(&needle_folded))
            .into_iter()
            .map(|(start, end)| map_folded_span(&map, text.chars().count(), start, end))
            .collect()
    }

    fn check(text: &str, needle: &str, case_sensitive: bool) {
        assert_eq!(
            plain_ranges(text, needle, case_sensitive),
            oracle(text, needle, case_sensitive),
            "text {text:?} needle {needle:?} case_sensitive {case_sensitive}"
        );
    }

    fn marker_text() -> (String, usize) {
        let size = 1024 * 1024;
        let buf = TextBuffer::from_str(&"a".repeat(size));
        let mut acc = 0usize;
        let mut boundary = None;
        for chunk in buf.text().chunks() {
            let next = acc + chunk.len();
            if next < buf.text().len_bytes() {
                boundary = Some(next);
                break;
            }
            acc = next;
        }
        let boundary = boundary.expect("1 MiB rope has a chunk boundary");
        assert!(boundary > 0 && boundary + 1 < size);
        let mut bytes = vec![b'a'; size];
        bytes[boundary - 1] = b'Q';
        bytes[boundary] = b'Z';
        let text = String::from_utf8(bytes).expect("marker bytes are ASCII");
        assert!(
            marker_straddles(&text, boundary),
            "marker must straddle a rope chunk"
        );
        (text, boundary)
    }

    fn marker_straddles(text: &str, boundary: usize) -> bool {
        let buf = TextBuffer::from_str(text);
        let mut acc = 0usize;
        let mut q_chunk = None;
        let mut z_chunk = None;
        for (index, chunk) in buf.text().chunks().enumerate() {
            let next = acc + chunk.len();
            if (acc..next).contains(&(boundary - 1)) {
                q_chunk = Some(index);
            }
            if (acc..next).contains(&boundary) {
                z_chunk = Some(index);
            }
            acc = next;
        }
        q_chunk.is_some() && q_chunk != z_chunk
    }

    fn arb_char() -> impl Strategy<Value = char> {
        prop_oneof![
            8 => prop::char::range('a', 'z'),
            4 => prop::char::range('A', 'Z'),
            1 => prop::char::range('0', '9'),
            1 => Just(' '),
            1 => Just('\n'),
            1 => Just('_'),
            1 => Just('é'),
            1 => Just('É'),
            1 => Just('Σ'),
            1 => Just('İ'),
            1 => Just('你'),
            1 => Just('\''),
        ]
    }

    #[test]
    fn plain_search_agrees_with_str_match_indices() {
        check("Hello", "hello", true);
        check("Hello", "hello", false);
        check("éé", "é", true);
        check("aaa", "aa", true);
        check("aéb", "", true);
        check("aéb", "", false);
        check("ΣΣ", "σς", false);
        check("ΣΣ", "σσ", false);
        check("aΣ", "aς", false);
        check("İ", "i\u{0307}", false);
        check("İ", "i", false);
        check("İ", "", false);
        check("", "", true);
        check("", "a", false);

        let (text, boundary) = marker_text();
        check(&text, "QZ", true);
        assert_eq!(
            plain_ranges(&text, "QZ", true),
            vec![(boundary - 1, boundary + 1)]
        );

        let config = fixed_config();
        proptest!(config, |(text in prop::collection::vec(arb_char(), 0..24), needle in prop::collection::vec(arb_char(), 0..8), case_sensitive in any::<bool>())| {
            let text: String = text.into_iter().collect();
            let needle: String = needle.into_iter().collect();
            prop_assert_eq!(
                plain_ranges(&text, &needle, case_sensitive),
                oracle(&text, &needle, case_sensitive)
            );
        });
    }

    #[test]
    fn regex_over_chunk_boundary_matches() {
        let (text, boundary) = marker_text();
        let buf = TextBuffer::from_str(&text);
        let found = find_all(&buf, &query(Pattern::Regex("Q[A-Z]".into()), true, false))
            .expect("pattern compiles");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start(), Offset(boundary - 1));
        assert_eq!(found[0].range.end(), Offset(boundary + 1));
        assert_eq!(found[0].captures.len(), 1);
        assert_eq!(found[0].captures[0], Some(found[0].range));
        assert_eq!(buf.slice(found[0].range).to_string(), "QZ");
    }

    #[test]
    fn replace_all_is_one_transaction_and_undoes_in_one_step() {
        let original = "ab".repeat(1000);
        let mut buf = TextBuffer::from_str(&original);
        let (tx, count) = replace_all(&buf, &query(Pattern::Plain("ab".into()), true, false), "Z")
            .expect("plain replace does not fail");
        assert_eq!(count, 1000);
        assert_eq!(tx.edits().len(), 1000);
        assert_eq!(tx.edits()[0].start, Offset(0));
        assert_eq!(tx.edits()[0].old, "ab");
        assert_eq!(tx.edits()[0].new, "Z");
        assert_eq!(tx.edits()[999].start, Offset(1998));

        let revision = buf.revision();
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.revision().0, revision.0 + 1);
        assert_eq!(buf.text().to_string(), "Z".repeat(1000));
        buf.apply(&tx.invert())
            .expect("the inverse matches the new buffer");
        assert_eq!(buf.text().to_string(), original);
    }

    #[test]
    fn find_next_wraps_and_terminates() {
        let buf = TextBuffer::from_str("ab ab ab");
        let q = query(Pattern::Plain("ab".into()), true, false);
        let all = find_all(&buf, &q).expect("plain search does not fail");
        assert_eq!(all.len(), 3);

        let from_last = all[2].range.end();
        let wrapped = find_next(&buf, &q, from_last, false, true)
            .expect("plain search does not fail")
            .expect("wrap finds the first hit");
        assert_eq!(wrapped.range, all[0].range);
        assert_eq!(
            find_next(&buf, &q, from_last, false, false).expect("plain search does not fail"),
            None
        );

        let first = find_next(&buf, &q, Offset(0), false, false)
            .expect("plain search does not fail")
            .expect("the hit at the cursor is returned");
        assert_eq!(first.range, all[0].range);

        let back = find_next(&buf, &q, Offset(0), true, true)
            .expect("plain search does not fail")
            .expect("wrap finds the last hit");
        assert_eq!(back.range, all[2].range);
        assert_eq!(
            find_next(&buf, &q, Offset(0), true, false).expect("plain search does not fail"),
            None
        );

        let absent = query(Pattern::Plain("nope".into()), true, false);
        assert_eq!(
            find_next(&buf, &absent, Offset(0), false, true).expect("plain search does not fail"),
            None
        );
        assert_eq!(
            find_next(&buf, &absent, Offset(2), true, true).expect("plain search does not fail"),
            None
        );

        let sample = TextBuffer::from_str("xyz");
        let empty_q = query(Pattern::Regex("a*".into()), true, false);
        let mut from = Offset(0);
        let mut steps = 0;
        loop {
            assert!(steps < 8, "find_next did not terminate");
            let found =
                find_next(&sample, &empty_q, from, false, false).expect("the regex compiles");
            let Some(hit) = found else {
                break;
            };
            assert!(
                !(hit.range.is_empty() && hit.range.start() == from),
                "empty match at from"
            );
            let next = hit.range.end().0 + usize::from(hit.range.is_empty());
            assert!(next > from.0, "the cursor did not advance");
            from = Offset(next);
            steps += 1;
        }
        assert!(steps > 0, "a* reports an empty hit");
    }

    #[test]
    fn escape_sequences_expand_before_a_plain_search() {
        let buf = TextBuffer::from_str("a\tb\nA");
        let tab = find_all(
            &buf,
            &query(Pattern::EscapeSequences(r"\t".into()), true, false),
        )
        .expect("tab escape is valid");
        assert_eq!(tab.len(), 1);
        assert_eq!(tab[0].range, CharRange::new(Offset(1), Offset(2)));
        assert!(tab[0].captures.is_empty());

        let hex = find_all(
            &buf,
            &query(Pattern::EscapeSequences(r"\x41".into()), true, false),
        )
        .expect("hex escape is valid");
        assert_eq!(hex[0].range.start(), Offset(4));

        let uni = find_all(
            &buf,
            &query(Pattern::EscapeSequences(r"\u0041".into()), true, false),
        )
        .expect("unicode escape is valid");
        assert_eq!(uni[0].range, hex[0].range);

        let slash = TextBuffer::from_str(r"a\b");
        let found = find_all(
            &slash,
            &query(Pattern::EscapeSequences(r"\\".into()), true, false),
        )
        .expect("backslash escape is valid");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start(), Offset(1));

        assert_eq!(
            find_all(
                &buf,
                &query(Pattern::EscapeSequences(r"\q".into()), true, false)
            ),
            Err(SearchError::InvalidEscape { at: 0 })
        );
        assert_eq!(
            find_all(
                &buf,
                &query(Pattern::EscapeSequences(r"\r".into()), true, false)
            ),
            Err(SearchError::InvalidEscape { at: 0 })
        );
        assert!(matches!(
            find_all(
                &buf,
                &query(Pattern::EscapeSequences(r"\x4".into()), true, false)
            ),
            Err(SearchError::InvalidEscape { .. })
        ));
        assert!(matches!(
            find_all(
                &buf,
                &query(Pattern::EscapeSequences(r"\uD800".into()), true, false)
            ),
            Err(SearchError::InvalidEscape { .. })
        ));
    }

    #[test]
    fn replace_all_expands_dollar_and_backslash_captures() {
        let mut buf = TextBuffer::from_str("ab ab");
        let q = query(Pattern::Regex(r"(?<w>a)(b)".into()), true, false);
        let before = find_all(&buf, &q).expect("pattern compiles");
        assert_eq!(before[0].captures.len(), 3);
        let expanded = expand_replacement(r"$2${w}", &before[0], &buf);
        assert_eq!(expanded, "b${w}");

        let (tx, count) = replace_all(&buf, &q, r"$2${w}-\1\n").expect("pattern compiles");
        assert_eq!(count, 2);
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.text().to_string(), "ba-a\n ba-a\n");
    }

    #[test]
    fn replace_all_over_100k_matches_stays_fast() {
        let count_hits = 100_000;
        let original = "a".repeat(count_hits);
        let buf = TextBuffer::from_str(&original);
        let started = std::time::Instant::now();
        let (tx, count) = replace_all(&buf, &query(Pattern::Plain("a".into()), true, false), "b")
            .expect("plain replace does not fail");
        let elapsed = started.elapsed();
        assert_eq!(count, count_hits);
        assert_eq!(tx.edits().len(), count_hits);
        assert_eq!(tx.edits()[0].start, Offset(0));
        assert_eq!(tx.edits()[count_hits - 1].start, Offset(count_hits - 1));
        assert!(
            elapsed.as_secs() < 15,
            "replace_all of {count_hits} hits took {elapsed:?}"
        );
    }

    #[test]
    fn plain_replace_all_keeps_the_replacement_literal() {
        let mut buf = TextBuffer::from_str("go");
        let replacement = r"$5 \1 C:\path";
        let (tx, count) = replace_all(
            &buf,
            &query(Pattern::Plain("go".into()), true, false),
            replacement,
        )
        .expect("plain replace does not fail");
        assert_eq!(count, 1);
        assert_eq!(tx.edits()[0].new, replacement);
        buf.apply(&tx).expect("the transaction matches the buffer");
        assert_eq!(buf.text().to_string(), replacement);
    }

    #[test]
    fn whole_word_keeps_a_hit_that_overlaps_a_rejected_candidate() {
        let buf = TextBuffer::from_str("ba-a-a");
        let found = find_all(&buf, &query(Pattern::Plain("a-a".into()), true, true))
            .expect("plain search does not fail");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start(), Offset(3));
        assert_eq!(found[0].range.end(), Offset(6));

        let folded = find_all(&buf, &query(Pattern::Plain("A-a".into()), false, true))
            .expect("plain search does not fail");
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].range.start(), Offset(3));
        assert_eq!(folded[0].range.end(), Offset(6));
    }

    #[test]
    fn whole_word_treats_accented_letters_as_word_chars() {
        let inside = TextBuffer::from_str("aéa");
        let blocked = find_all(&inside, &query(Pattern::Plain("é".into()), true, true))
            .expect("plain search does not fail");
        assert!(blocked.is_empty());

        let beside = TextBuffer::from_str("a é b");
        let found = find_all(&beside, &query(Pattern::Plain("é".into()), true, true))
            .expect("plain search does not fail");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range.start(), Offset(2));
        assert_eq!(found[0].range.end(), Offset(3));
    }

    #[test]
    fn whole_words_use_boundaries_and_regex_ignores_the_flag() {
        let buf = TextBuffer::from_str("Cat catalog CAT");
        let found = find_all(&buf, &query(Pattern::Plain("cat".into()), false, true))
            .expect("plain search does not fail");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].range.start(), Offset(0));
        assert_eq!(found[1].range.start(), Offset(12));
        assert!(
            find_all(&buf, &query(Pattern::Plain("cat".into()), true, true))
                .expect("plain search does not fail")
                .is_empty()
        );

        let loose = find_all(&buf, &query(Pattern::Regex("cat".into()), false, false))
            .expect("pattern compiles");
        let strict = find_all(&buf, &query(Pattern::Regex("cat".into()), false, true))
            .expect("pattern compiles");
        assert_eq!(loose, strict);
        assert!(loose.len() > 2);
    }

    #[test]
    fn bad_input_returns_an_error() {
        let buf = TextBuffer::from_str("ab");
        let bad = query(Pattern::Regex("(".into()), true, false);
        assert!(matches!(
            find_all(&buf, &bad),
            Err(SearchError::InvalidRegex(_))
        ));
        assert!(matches!(
            replace_all(&buf, &bad, "x"),
            Err(SearchError::InvalidRegex(_))
        ));

        let q = query(Pattern::Plain("a".into()), true, false);
        let hit = find_next(&buf, &q, Offset(usize::MAX), false, true)
            .expect("plain search does not fail");
        assert_eq!(hit.expect("wrap finds the hit").range.start(), Offset(0));

        let wide = SearchQuery {
            range: Some(CharRange::new(Offset(0), Offset(50))),
            ..query(Pattern::Plain("ab".into()), true, false)
        };
        assert_eq!(
            find_all(&buf, &wide)
                .expect("plain search does not fail")
                .len(),
            1
        );

        let outside = SearchQuery {
            range: Some(CharRange::new(Offset(10), Offset(12))),
            ..query(Pattern::Plain("a".into()), true, false)
        };
        assert!(
            find_all(&buf, &outside)
                .expect("plain search does not fail")
                .is_empty()
        );

        let empty = TextBuffer::from_str("");
        let (tx, count) = replace_all(
            &empty,
            &query(Pattern::Plain("z".into()), true, false),
            r"$1\",
        )
        .expect("plain replace does not fail");
        assert_eq!(count, 0);
        assert!(tx.is_empty());

        let blank = Match {
            range: CharRange::new(Offset(0), Offset(0)),
            captures: Vec::new(),
        };
        assert_eq!(expand_replacement("$", &blank, &empty), "$");
        assert_eq!(expand_replacement("\\", &blank, &empty), "\\");
        assert_eq!(expand_replacement(r"\n\t", &blank, &empty), "\n\t");
        assert_eq!(expand_replacement("$$", &blank, &empty), "$");

        let empty_re = find_all(&buf, &query(Pattern::Regex(String::new()), true, false))
            .expect("an empty regex compiles");
        assert!(empty_re.len() <= 4);
    }
}
