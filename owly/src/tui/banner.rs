//! Directory display helpers for Owly TUI chrome.

use std::path::Path;

use crate::cli::truncate_path_for_display;

pub fn directory_display(cwd: &Path) -> String {
    truncate_path_for_display(cwd, 48)
}
