//! Image tool — read and describe images from the filesystem.
//!
//! Ported from pi-agent-core's `harness/tools/image.ts`.
//! Reads image metadata and returns information about the image.

use std::sync::Arc;

use elph_ai::Tool;
use serde_json::json;

use crate::agent::harness::types::{FileSystem, Result as HarnessResult};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::tools::types::context_aware_tool;
use crate::types::AgentTool;

/// Create an image tool that reads image metadata.
///
/// Supports PNG, JPG, JPEG, GIF, WebP, BMP, and SVG formats.
pub fn create_image_tool(env: Arc<LocalExecutionEnv>) -> AgentTool {
    let _ = env;
    context_aware_tool(
        Tool {
            name: "image".into(),
            constrained_sampling: None,
            description: "Read and get information about images from the filesystem. \
                         Supports PNG, JPG, JPEG, GIF, WebP, BMP, and SVG formats."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the image file (relative or absolute)"
                    }
                },
                "required": ["path"]
            }),
        },
        "image",
        |_id, args, _signal, _on_update, context| {
            Box::pin(async move {
                let result: anyhow::Result<crate::types::AgentToolResult> = async {
                    let path = args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing required argument: path"))?;

                    let resolved = match FileSystem::absolute_path(&*context.env, path, None).await {
                        HarnessResult::Ok(p) => p,
                        HarnessResult::Err(e) => anyhow::bail!("Failed to resolve path: {}", e.message),
                    };

                    let path_buf = std::path::Path::new(&resolved);
                    if !path_buf.exists() {
                        anyhow::bail!("File not found: {path}");
                    }

                    let metadata = tokio::fs::metadata(path_buf)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to read metadata: {e}"))?;

                    if !metadata.is_file() {
                        anyhow::bail!("Path is not a file: {path}");
                    }

                    let file_size = metadata.len();
                    let extension = path_buf
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    let is_image =
                        matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg");

                    if !is_image {
                        anyhow::bail!("File is not a supported image format: .{extension}");
                    }

                    Ok(crate::types::AgentToolResult {
                        content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(
                            format!(
                                "Image file: {path}\nSize: {} bytes\nFormat: {}\nResolved path: {resolved}",
                                file_size,
                                extension.to_uppercase(),
                            ),
                        ))],
                        details: json!({
                            "path": path,
                            "resolved": resolved,
                            "size": file_size,
                            "format": extension,
                        }),
                        added_tool_names: None,
                        terminate: None,
                        usage: None,
                    })
                }
                .await;
                result.map_err(crate::tools::types::ToolError::from)
            })
        },
    )
}
