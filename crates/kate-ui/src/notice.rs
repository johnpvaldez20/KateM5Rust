// SPDX-FileCopyrightText: 2026 John Valdez
// SPDX-License-Identifier: GPL-3.0-or-later

//! One sentence for each command error.

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
