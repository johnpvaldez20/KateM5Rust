// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON frame lines on stderr.

use crate::FrameSample;

/// Serialize one sample. The caller prints it when profiling is on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn sample_json(sample: &FrameSample) -> String {
    serde_json::to_string(sample).expect("frame sample")
}

/// Write one JSON line to stderr when `KATEM5_PROFILE` is `1`.
#[cfg(target_os = "macos")]
pub(crate) fn write_sample(sample: &FrameSample) {
    let enabled =
        std::env::var_os("KATEM5_PROFILE").is_some_and(|value| value == std::ffi::OsStr::new("1"));
    if enabled {
        eprintln!("{}", sample_json(sample));
    }
}

#[cfg(test)]
mod tests {
    use super::sample_json;
    use crate::FrameSample;

    #[test]
    fn frame_sample_json_uses_field_names() {
        let line = sample_json(&FrameSample {
            frame: 1,
            paint_micros: 2,
            first_line: 3,
            lines_painted: 4,
            highlight_micros: 5,
            shape_micros: 6,
            draw_micros: 7,
        });
        assert_eq!(
            line,
            r#"{"frame":1,"paint_micros":2,"first_line":3,"lines_painted":4,"highlight_micros":5,"shape_micros":6,"draw_micros":7}"#
        );
    }
}
