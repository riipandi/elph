//! Agent tools for codegraph (search / impact / status / dirty reindex).
//! Full build and purge are CLI-only.

use std::sync::Arc;

use anyhow::{Context, Result};
use elph_agent::{AgentTool, AgentToolResult};
use elph_ai::Tool;
use serde_json::{Value, json};

use super::store::open_store;
use crate::platform::Paths;
use floppy::SearchOptions;

/// Create agent-facing codegraph tools (no build/purge).
pub fn create_codegraph_tools(paths: Paths) -> Vec<AgentTool> {
    let paths = Arc::new(paths);
    vec![
        create_search_tool(Arc::clone(&paths)),
        create_impact_tool(Arc::clone(&paths)),
        create_status_tool(Arc::clone(&paths)),
        create_reindex_tool(paths),
    ]
}

fn create_search_tool(paths: Arc<Paths>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "code_search".into(),
            constrained_sampling: None,
            description: "Semantic + keyword search over the project code index. \
                          Returns path, line range, and snippet so you can open only the relevant region \
                          instead of scanning the whole repo. Prefer this before broad ripgrep/glob. \
                          If the index is empty, tell the user to run `elph codegraph build`."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query (function name, identifier, or natural language)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max hits (default 10, max 50)"
                    }
                },
                "required": ["query"]
            }),
        },
        "code_search",
        move |_, args| {
            let paths = Arc::clone(&paths);
            Box::pin(async move { execute_search(paths, args).await })
        },
    )
}

async fn execute_search(paths: Arc<Paths>, args: Value) -> Result<AgentToolResult> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`query` is required"))?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(10)
        .clamp(1, 50);

    let store = open_store(&paths, true).context("open codegraph store")?;
    let status = store.status().await?;
    if status.file_count == 0 {
        let text = "Code index is empty. Ask the user to run: elph codegraph build";
        return Ok(AgentToolResult {
            content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
            details: json!({ "empty": true }),
            added_tool_names: None,
            terminate: None,
            usage: None,
        });
    }

    let hits = store
        .search(SearchOptions {
            query: query.to_string(),
            limit,
            refresh_dirty: true,
        })
        .await?;

    let list: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "path": h.path,
                "name": h.name,
                "kind": h.kind,
                "startLine": h.start_line,
                "endLine": h.end_line,
                "score": h.score,
                "source": h.source,
                "snippet": h.snippet,
            })
        })
        .collect();

    let text = if hits.is_empty() {
        format!("No codegraph matches for {query:?}.")
    } else {
        let mut lines = vec![format!("Found {} hit(s) for {query:?}:", hits.len())];
        for h in &hits {
            let name = h.name.as_deref().unwrap_or("-");
            lines.push(format!(
                "- {}:{}-{} {} [{}] ({:.3})",
                h.path, h.start_line, h.end_line, name, h.kind, h.score
            ));
            lines.push(format!("  {}", h.snippet.lines().next().unwrap_or("")));
        }
        lines.join("\n")
    };

    Ok(AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({ "hits": list }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

fn create_impact_tool(paths: Arc<Paths>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "code_impact".into(),
            constrained_sampling: None,
            description: "Shallow impact / neighbor lookup in the code graph for a path or symbol \
                          (import edges). Use after locating a symbol with code_search."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "File path, symbol name, or node id"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "BFS depth (default 1, max 4)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max nodes (default 30)"
                    }
                },
                "required": ["target"]
            }),
        },
        "code_impact",
        move |_, args| {
            let paths = Arc::clone(&paths);
            Box::pin(async move { execute_impact(paths, args).await })
        },
    )
}

async fn execute_impact(paths: Arc<Paths>, args: Value) -> Result<AgentToolResult> {
    let target = args
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`target` is required"))?;
    let depth = args.get("depth").and_then(Value::as_u64).map(|n| n as u32).unwrap_or(1);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(30);

    let store = open_store(&paths, false)?;
    let nodes = store.impact(target, depth, limit).await?;
    let list: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "path": n.path,
                "name": n.name,
                "kind": n.kind,
                "depth": n.depth,
            })
        })
        .collect();

    let text = if nodes.is_empty() {
        format!("No impact graph nodes for {target:?}.")
    } else {
        let mut lines = vec![format!("{} node(s) for {target:?}:", nodes.len())];
        for n in &nodes {
            lines.push(format!(
                "- d{} {} {} ({})",
                n.depth,
                n.path,
                n.name.as_deref().unwrap_or("-"),
                n.kind
            ));
        }
        lines.join("\n")
    };

    Ok(AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({ "nodes": list }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

fn create_status_tool(paths: Arc<Paths>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "code_status".into(),
            constrained_sampling: None,
            description: "Show codegraph index status (file/chunk counts, merkle fingerprint). \
                          If empty, the user must run `elph codegraph build` (CLI-only)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "code_status",
        move |_, _args| {
            let paths = Arc::clone(&paths);
            Box::pin(async move { execute_status(paths).await })
        },
    )
}

async fn execute_status(paths: Arc<Paths>) -> Result<AgentToolResult> {
    let store = open_store(&paths, false)?;
    let st = store.status().await?;
    let text = format!(
        "codegraph: files={} chunks={} nodes={} edges={} merkle={} empty={}",
        st.file_count,
        st.chunk_count,
        st.node_count,
        st.edge_count,
        st.merkle_root.as_deref().unwrap_or("-"),
        st.file_count == 0
    );
    Ok(AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "fileCount": st.file_count,
            "chunkCount": st.chunk_count,
            "nodeCount": st.node_count,
            "edgeCount": st.edge_count,
            "merkleRoot": st.merkle_root,
            "lastIndexedAt": st.last_indexed_at,
            "empty": st.file_count == 0,
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

fn create_reindex_tool(paths: Arc<Paths>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "code_reindex".into(),
            constrained_sampling: None,
            description: "Incrementally reindex changed files in the codegraph (dirty update only). \
                          Does not perform a full rebuild — full build is CLI-only: elph codegraph build."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "code_reindex",
        move |_, _args| {
            let paths = Arc::clone(&paths);
            Box::pin(async move { execute_reindex(paths).await })
        },
    )
}

async fn execute_reindex(paths: Arc<Paths>) -> Result<AgentToolResult> {
    let store = open_store(&paths, true)?;
    let stats = store.update().await?;
    let text = format!(
        "reindex done: walked={} indexed={} unchanged={} chunks={} (walk {}ms · reindex {}ms · finalize {}ms)",
        stats.files_walked,
        stats.files_indexed,
        stats.files_unchanged,
        stats.chunks_indexed,
        stats.walk_ms,
        stats.reindex_ms,
        stats.finalize_ms
    );
    Ok(AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "filesWalked": stats.files_walked,
            "filesIndexed": stats.files_indexed,
            "filesUnchanged": stats.files_unchanged,
            "chunksIndexed": stats.chunks_indexed,
            "chunksEmbedded": stats.chunks_embedded,
            "walkMs": stats.walk_ms,
            "reindexMs": stats.reindex_ms,
            "finalizeMs": stats.finalize_ms,
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}
