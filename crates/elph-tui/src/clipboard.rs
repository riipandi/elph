//! System clipboard helpers backed by [`clipboard_rs`].
//!
//! Replaces ad-hoc `pbcopy` / `xclip` process spawns with a cross-platform native API.

use anyhow::{Result, bail};
use clipboard_rs::{Clipboard, ClipboardContext};

/// Copy plain text to the system clipboard.
///
/// Empty strings are accepted (clears to empty clipboard content where supported).
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let ctx = ClipboardContext::new().map_err(|err| anyhow::anyhow!("open system clipboard: {err}"))?;
    ctx.set_text(text.to_string())
        .map_err(|err| anyhow::anyhow!("set clipboard text: {err}"))
}

/// Read plain text from the system clipboard.
pub fn read_from_clipboard() -> Result<String> {
    let ctx = ClipboardContext::new().map_err(|err| anyhow::anyhow!("open system clipboard: {err}"))?;
    ctx.get_text()
        .map_err(|err| anyhow::anyhow!("get clipboard text: {err}"))
}

/// Copy text and return a short human status for a11y / toast banners.
///
/// `label` is a noun phrase such as `"selection"` or `"prompt"`.
pub fn copy_with_status(text: &str, label: &str) -> Result<ClipboardCopyStatus> {
    if text.is_empty() {
        bail!("nothing to copy");
    }
    copy_to_clipboard(text)?;
    Ok(ClipboardCopyStatus {
        char_count: text.chars().count(),
        label: label.to_string(),
    })
}

/// Result metadata after a successful clipboard write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCopyStatus {
    pub char_count: usize,
    pub label: String,
}

impl ClipboardCopyStatus {
    /// Plain-language announcement (screen-reader / status row friendly).
    pub fn announcement(&self) -> String {
        let unit = if self.char_count == 1 { "char" } else { "chars" };
        format!("Copied {} ({} {})", self.label, self.char_count, unit)
    }
}

/// One-shot notice for parents to surface as an ephemeral toast.
///
/// Written by [`crate::components::Textarea`] on yank/copy; drained by the shell each frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardNotice {
    /// Selection (or other labeled payload) copied successfully.
    Copied { label: String, char_count: usize },
    /// Clipboard write failed.
    Failed { detail: String },
}

impl ClipboardNotice {
    pub fn selection_copied(char_count: usize) -> Self {
        Self::Copied {
            label: "selection".into(),
            char_count,
        }
    }

    pub fn failed(detail: impl Into<String>) -> Self {
        Self::Failed { detail: detail.into() }
    }

    /// Plain-language line for ephemeral banners / screen readers.
    pub fn announcement(&self) -> String {
        match self {
            Self::Copied { label, char_count } => ClipboardCopyStatus {
                char_count: *char_count,
                label: label.clone(),
            }
            .announcement(),
            Self::Failed { .. } => "Could not copy to clipboard · check clipboard access".into(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_status_announcement_is_plain_text() {
        let status = ClipboardCopyStatus {
            char_count: 12,
            label: "selection".into(),
        };
        assert_eq!(status.announcement(), "Copied selection (12 chars)");
        let one = ClipboardCopyStatus {
            char_count: 1,
            label: "selection".into(),
        };
        assert_eq!(one.announcement(), "Copied selection (1 char)");
    }

    #[test]
    fn copy_with_status_rejects_empty() {
        assert!(copy_with_status("", "selection").is_err());
    }

    #[test]
    fn selection_notice_announcement() {
        let ok = ClipboardNotice::selection_copied(4);
        assert!(!ok.is_error());
        assert_eq!(ok.announcement(), "Copied selection (4 chars)");
        assert!(ClipboardNotice::failed("x").is_error());
    }
}
