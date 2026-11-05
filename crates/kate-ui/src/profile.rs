// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON frame lines on stderr.

use crate::FrameSample;

/// Serialize one sample. The caller prints it when profiling is on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn sample_json(sample: &FrameSample) -> String {
    serde_json::to_string(sample).expect("frame sample")
}

/// Return true when the profiling environment variable is `1` or any ASCII case of `true`.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
fn profile_is_on(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| {
        value == std::ffi::OsStr::new("1") || value.to_string_lossy().eq_ignore_ascii_case("true")
    })
}

/// Write one JSON line to stderr when `KATEM5_PROFILE` is `1` or `true`.
#[cfg(target_os = "macos")]
pub(crate) fn write_sample(sample: &FrameSample) {
    if profile_is_on(std::env::var_os("KATEM5_PROFILE").as_deref()) {
        eprintln!("{}", sample_json(sample));
    }
}

#[cfg(test)]
mod tests {
    use super::{profile_is_on, sample_json};
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

    #[test]
    fn profile_is_on_for_one_and_true() {
        assert!(profile_is_on(Some(std::ffi::OsStr::new("1"))));
        assert!(profile_is_on(Some(std::ffi::OsStr::new("true"))));
        assert!(!profile_is_on(Some(std::ffi::OsStr::new("yes"))));
        assert!(!profile_is_on(Some(std::ffi::OsStr::new("0"))));
        assert!(!profile_is_on(None));
    }

    #[test]
    fn profile_is_on_for_upper_true() {
        assert!(profile_is_on(Some(std::ffi::OsStr::new("TRUE"))));
        assert!(profile_is_on(Some(std::ffi::OsStr::new("True"))));
        assert!(!profile_is_on(Some(std::ffi::OsStr::new("yes"))));
    }
}
