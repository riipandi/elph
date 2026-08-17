//! Prompt content conversion (ACP v2 → Elph text / images).

use agent_client_protocol::schema::v2::{ContentBlock, EmbeddedResourceResource};

pub struct ExtractedPrompt {
    pub text: String,
    pub images: Vec<(String, String)>,
}

pub fn extract_prompt(blocks: &[ContentBlock]) -> Result<ExtractedPrompt, String> {
    let mut parts = Vec::new();
    let mut images = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text(text) => {
                if !text.text.is_empty() {
                    parts.push(text.text.clone());
                }
            }
            ContentBlock::ResourceLink(link) => {
                let mut line = format!("[resource {}]({})", link.name, link.uri);
                if let Some(mime) = &link.mime_type {
                    line.push_str(&format!(" ({mime})"));
                }
                if let Some(desc) = &link.description {
                    line.push_str(&format!(" — {desc}"));
                }
                parts.push(line);
            }
            ContentBlock::Resource(embedded) => match &embedded.resource {
                EmbeddedResourceResource::TextResourceContents(res) => {
                    parts.push(format!("<resource uri=\"{}\">\n{}\n</resource>", res.uri, res.text));
                }
                EmbeddedResourceResource::BlobResourceContents(res) => {
                    parts.push(format!("<resource uri=\"{}\" blob_bytes={}/>", res.uri, res.blob.len()));
                }
                _ => {}
            },
            ContentBlock::Image(image) => {
                images.push((image.data.clone(), image.mime_type.to_string()));
                if let Some(uri) = &image.uri {
                    parts.push(format!("[image]({uri})"));
                } else {
                    parts.push("[image attached]".to_string());
                }
            }
            ContentBlock::Audio(_) => {
                return Err("audio content is not supported".into());
            }
            ContentBlock::Other(other) if !other.type_.starts_with('_') => {
                return Err(format!("unsupported content type '{}'", other.type_));
            }
            ContentBlock::Other(_) => {}
            _ => {}
        }
    }

    Ok(ExtractedPrompt {
        text: parts.join("\n"),
        images,
    })
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v2::{ResourceLink, TextContent};

    use super::*;

    #[test]
    fn extracts_text_and_resource_link() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("hello")),
            ContentBlock::ResourceLink(ResourceLink::new("doc.md", "file:///tmp/doc.md")),
        ];
        let extracted = extract_prompt(&blocks).unwrap();
        assert!(extracted.text.contains("hello"));
        assert!(extracted.text.contains("file:///tmp/doc.md"));
    }

    #[test]
    fn rejects_audio() {
        use agent_client_protocol::schema::v2::AudioContent;
        let blocks = vec![ContentBlock::Audio(AudioContent::new("AAAA", "audio/wav"))];
        assert!(extract_prompt(&blocks).is_err());
    }
}
