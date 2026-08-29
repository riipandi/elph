//! System clipboard helpers backed by [`clipboard_rs`].
//!
//! Replaces ad-hoc `pbcopy` / `xclip` process spawns with a cross-platform native API.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext};

static IMAGE_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A clipboard image staged for the next prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    pub id: usize,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

/// State shown by the prompt while a clipboard image is being staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImagePasteDialogState {
    Loading,
    Preview(ImageAttachment),
    Unsupported,
    Failed(String),
}

/// Probe whether the system clipboard can be opened (no read/write).
pub fn clipboard_available() -> bool {
    ClipboardContext::new().is_ok()
}

/// Short backend label for doctor / diagnostics (no secrets).
pub fn clipboard_backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "native (clipboard_rs / pbcopy)"
    } else {
        "native (clipboard_rs)"
    }
}

/// Copy plain text to the system clipboard.
///
/// Empty strings are accepted (clears to empty clipboard content where supported).
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let ctx = ClipboardContext::new().map_err(|err| {
        log::warn!("clipboard open failed: {err}");
        anyhow::anyhow!("open system clipboard: {err}")
    })?;
    ctx.set_text(text.to_string()).map_err(|err| {
        log::warn!("clipboard write failed: {err}");
        anyhow::anyhow!("set clipboard text: {err}")
    })?;
    log::debug!("clipboard write ok chars={}", text.chars().count());
    Ok(())
}

/// Read plain text from the system clipboard.
pub fn read_from_clipboard() -> Result<String> {
    let ctx = ClipboardContext::new().map_err(|err| {
        log::warn!("clipboard open failed: {err}");
        anyhow::anyhow!("open system clipboard: {err}")
    })?;
    ctx.get_text().map_err(|err| {
        log::warn!("clipboard read failed: {err}");
        anyhow::anyhow!("get clipboard text: {err}")
    })
}

/// Save the image currently on the system clipboard as a PNG attachment.
pub fn save_clipboard_image(dir: &Path, id: usize) -> Result<ImageAttachment> {
    let ctx = ClipboardContext::new().map_err(|err| anyhow::anyhow!("open system clipboard: {err}"))?;
    let image = ctx
        .get_image()
        .map_err(|err| anyhow::anyhow!("read clipboard image: {err}"))?;
    if image.is_empty() {
        bail!("clipboard image is empty");
    }

    std::fs::create_dir_all(dir)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = IMAGE_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = reserve_image_path(dir, timestamp, counter)?;
    if let Err(err) = image.save_to_path(&path.to_string_lossy()) {
        let _ = std::fs::remove_file(&path);
        return Err(anyhow::anyhow!("save clipboard image: {err}"));
    }
    let (width, height) = image.get_size();
    Ok(ImageAttachment {
        id,
        path,
        width,
        height,
    })
}

fn reserve_image_path(dir: &Path, timestamp: u128, counter: u64) -> Result<PathBuf> {
    for suffix in 0..u64::MAX {
        let name = if suffix == 0 {
            format!("clipboard-{timestamp}-{counter}.png")
        } else {
            format!("clipboard-{timestamp}-{counter}-{suffix}.png")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    bail!("could not allocate a unique clipboard image path")
}

/// Remove staged attachment files. Missing files are already consumed/cleaned.
pub fn remove_image_attachments(attachments: &[ImageAttachment]) {
    for attachment in attachments {
        if let Err(err) = std::fs::remove_file(&attachment.path)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            log::debug!("remove image attachment {}: {err}", attachment.path.display());
        }
    }
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

    #[test]
    fn reserve_image_path_never_reuses_an_existing_attachment() {
        let dir = std::env::temp_dir().join(format!(
            "elph-clipboard-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temporary attachment directory");
        let existing = dir.join("clipboard-7-3.png");
        std::fs::write(&existing, b"existing").expect("write existing attachment");

        let reserved = reserve_image_path(&dir, 7, 3).expect("reserve attachment path");
        assert_ne!(reserved, existing);
        assert!(reserved.exists());

        std::fs::remove_dir_all(dir).expect("remove temporary attachment directory");
    }
}
