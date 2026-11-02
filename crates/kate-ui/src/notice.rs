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
        kate_core::CommandError::Apply(_) => "The edit was rejected.",
        kate_core::CommandError::Transaction(_) => "The edits overlap.",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::command_notice;
    use kate_core::CommandError;

    #[test]
    fn no_path_names_the_missing_file() {
        let no_file = command_notice(&CommandError::NoFile);
        let already_open = command_notice(&CommandError::AlreadyOpen);
        assert_eq!(no_file, "This document has no path.");
        assert_eq!(already_open, "Another document has that path.");
        assert_ne!(no_file, already_open);
    }
}
