// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! One sentence for each command error.

/// Return one sentence for a command error.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn command_notice(error: &kate_core::CommandError) -> String {
    match error {
        kate_core::CommandError::NoFile => "This document has no path.",
        kate_core::CommandError::AlreadyOpen => "Another document has that path.",
        kate_core::CommandError::UnknownView(_) => "The view is not open.",
        kate_core::CommandError::Save(_) => "The save failed.",
        kate_core::CommandError::Load(_) => "The load failed.",
        kate_core::CommandError::Apply(kate_text::ApplyError::OutOfBounds { .. }) => {
            "The edit is past the end."
        }
        kate_core::CommandError::Apply(kate_text::ApplyError::OldTextMismatch { .. }) => {
            "The edit does not match the buffer."
        }
        kate_core::CommandError::Transaction(_) => "The edits overlap.",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::command_notice;
    use kate_core::CommandError;
    use kate_text::{ApplyError, Offset};

    #[test]
    fn no_path_names_the_missing_file() {
        let no_file = command_notice(&CommandError::NoFile);
        let already_open = command_notice(&CommandError::AlreadyOpen);
        assert_eq!(no_file, "This document has no path.");
        assert_eq!(already_open, "Another document has that path.");
        assert_ne!(no_file, already_open);
    }

    #[test]
    fn an_out_of_bounds_edit_names_the_end() {
        let oob = ApplyError::OutOfBounds {
            edit: 0,
            end: Offset(3),
            len: Offset(1),
        };
        assert_eq!(
            command_notice(&CommandError::Apply(oob)),
            "The edit is past the end."
        );

        let mismatch = ApplyError::OldTextMismatch {
            edit: 0,
            expected: "a".to_owned(),
            found: "b".to_owned(),
        };
        assert_eq!(
            command_notice(&CommandError::Apply(mismatch)),
            "The edit does not match the buffer."
        );

        let no_file = command_notice(&CommandError::NoFile);
        assert_eq!(no_file, "This document has no path.");
    }
}
