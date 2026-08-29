//! Conversion and lifecycle helpers for clipboard image attachments.

use anyhow::{Context, Result};
use base64::Engine;

/// Convert staged PNG files into the model's image content blocks.
pub(crate) fn read_image_contents(attachments: &[elph_tui::ImageAttachment]) -> Result<Vec<elph_ai::ImageContent>> {
    attachments
        .iter()
        .map(|attachment| {
            let bytes = std::fs::read(&attachment.path)
                .with_context(|| format!("read image attachment {}", attachment.path.display()))?;
            Ok(elph_ai::ImageContent::new(
                base64::engine::general_purpose::STANDARD.encode(bytes),
                "image/png",
            ))
        })
        .collect()
}

/// Remove staged files after a prompt has either consumed or discarded them.
pub(crate) fn remove_files(attachments: &[elph_tui::ImageAttachment]) {
    elph_tui::remove_image_attachments(attachments);
}

#[cfg(test)]
mod tests {
    use super::read_image_contents;

    #[test]
    fn reads_png_attachment_as_image_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("image.png");
        std::fs::write(&path, b"png-bytes").expect("write");
        let attachments = [elph_tui::ImageAttachment {
            id: 1,
            path,
            width: 1,
            height: 1,
        }];

        let images = read_image_contents(&attachments).expect("image content");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].mime_type, "image/png");
        assert_eq!(images[0].data, "cG5nLWJ5dGVz");
    }
}
