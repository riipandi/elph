//! Hybrid FTS + vector search with RRF merge.

use anyhow::Result;
use std::collections::HashMap;
use turso::{Connection, params};

use super::migrations::fts_available;
use super::types::{ChunkHit, ImpactNode, SearchOptions};
use crate::core::embed::EmbedFn;
use crate::core::util::{drain_rows, is_zero, vec_buf};

const RRF_K: f64 = 60.0;

pub async fn hybrid_search(conn: &Connection, embed: &EmbedFn, opts: &SearchOptions) -> Result<Vec<ChunkHit>> {
    let limit = opts.limit.clamp(1, 50) as usize;
    let cand = (limit * 4).max(20);

    let mut ranks: HashMap<i64, (f64, ChunkHit)> = HashMap::new();

    // FTS path
    if fts_available(conn).await.unwrap_or(false) {
        let fts_q = sanitize_fts_query(&opts.query);
        if !fts_q.is_empty() {
            let sql = format!(
                "SELECT c.id, c.path, c.kind, c.name, c.start_line, c.end_line, c.content,
                        bm25(cg_fts) AS rank
                 FROM cg_fts
                 JOIN cg_chunks c ON c.id = cg_fts.rowid
                 WHERE cg_fts MATCH ?
                 ORDER BY rank
                 LIMIT {cand}"
            );
            if let Ok(mut rows) = conn.query(&sql, params![fts_q.as_str()]).await {
                let mut i = 0usize;
                while let Some(row) = rows.next().await? {
                    i += 1;
                    let id: i64 = row.get(0)?;
                    let rrf = 1.0 / (RRF_K + i as f64);
                    let hit = row_to_hit(&row, rrf, "fts")?;
                    ranks
                        .entry(id)
                        .and_modify(|(s, h)| {
                            *s += rrf;
                            h.score = *s;
                            if h.source == "vector" {
                                h.source = "both".into();
                            }
                        })
                        .or_insert((rrf, hit));
                }
                drain_rows(&mut rows).await?;
            }
        }
    } else {
        // LIKE fallback
        let like = format!("%{}%", opts.query.replace('%', "").replace('_', ""));
        let sql = format!(
            "SELECT id, path, kind, name, start_line, end_line, content, 0.0
             FROM cg_chunks
             WHERE content LIKE ? OR path LIKE ? OR IFNULL(name,'') LIKE ?
             LIMIT {cand}"
        );
        if let Ok(mut rows) = conn
            .query(&sql, params![like.as_str(), like.as_str(), like.as_str()])
            .await
        {
            let mut i = 0usize;
            while let Some(row) = rows.next().await? {
                i += 1;
                let id: i64 = row.get(0)?;
                let rrf = 1.0 / (RRF_K + i as f64);
                let hit = row_to_hit(&row, rrf, "like")?;
                ranks.insert(id, (rrf, hit));
            }
            drain_rows(&mut rows).await?;
        }
    }

    // Vector path
    let emb = (embed)(&opts.query).await?;
    if !is_zero(&emb) {
        let blob = vec_buf(&emb);
        let sql = format!(
            "SELECT id, path, kind, name, start_line, end_line, content,
                    vector_distance_cos(vector32(embedding), vector32(?)) AS distance
             FROM cg_chunks
             WHERE embedding IS NOT NULL
             ORDER BY distance ASC
             LIMIT {cand}"
        );
        if let Ok(mut rows) = conn.query(&sql, params![blob.as_slice()]).await {
            let mut i = 0usize;
            while let Some(row) = rows.next().await? {
                i += 1;
                let id: i64 = row.get(0)?;
                let rrf = 1.0 / (RRF_K + i as f64);
                let hit = row_to_hit(&row, rrf, "vector")?;
                ranks
                    .entry(id)
                    .and_modify(|(s, h)| {
                        *s += rrf;
                        h.score = *s;
                        if h.source == "fts" || h.source == "like" {
                            h.source = "both".into();
                        }
                    })
                    .or_insert((rrf, hit));
            }
            drain_rows(&mut rows).await?;
        }
    }

    let mut hits: Vec<ChunkHit> = ranks
        .into_values()
        .map(|(s, mut h)| {
            h.score = s;
            h
        })
        .collect();
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}

fn row_to_hit(row: &turso::Row, score: f64, source: &str) -> Result<ChunkHit> {
    let content: String = row.get(6)?;
    let snippet = snippet_of(&content, 240);
    Ok(ChunkHit {
        id: row.get(0)?,
        path: row.get(1)?,
        kind: row.get(2)?,
        name: row.get::<Option<String>>(3)?,
        start_line: row.get(4)?,
        end_line: row.get(5)?,
        score,
        snippet,
        source: source.to_string(),
    })
}

fn snippet_of(content: &str, max_chars: usize) -> String {
    let t = content.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    let s: String = t.chars().take(max_chars).collect();
    format!("{s}…")
}

/// Very small FTS query sanitizer: quote tokens, drop empty.
fn sanitize_fts_query(q: &str) -> String {
    let tokens: Vec<String> = q
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{cleaned}\"")
            }
        })
        .filter(|t| !t.is_empty())
        .collect();
    tokens.join(" ")
}

pub async fn impact(conn: &Connection, target: &str, max_depth: u32, limit: u32) -> Result<Vec<ImpactNode>> {
    // Resolve seed node(s)
    let seeds = resolve_seeds(conn, target).await?;
    if seeds.is_empty() {
        return Ok(Vec::new());
    }

    let mut seen: HashMap<String, ImpactNode> = HashMap::new();
    let mut frontier: Vec<(String, u32)> = seeds.into_iter().map(|id| (id, 0)).collect();

    while let Some((id, depth)) = frontier.pop() {
        if seen.contains_key(&id) || depth > max_depth {
            continue;
        }
        if let Some(node) = load_node(conn, &id).await? {
            let mut n = node;
            n.depth = depth;
            seen.insert(id.clone(), n);
        } else {
            // Synthetic import: target
            seen.insert(
                id.clone(),
                ImpactNode {
                    id: id.clone(),
                    path: id.trim_start_matches("import:").to_string(),
                    name: None,
                    kind: "import".into(),
                    depth,
                },
            );
        }
        if depth == max_depth {
            continue;
        }
        let mut rows = conn
            .query(
                "SELECT dst FROM cg_edges WHERE src = ?
                 UNION
                 SELECT src FROM cg_edges WHERE dst = ?",
                params![id.as_str(), id.as_str()],
            )
            .await?;
        while let Some(row) = rows.next().await? {
            let next: String = row.get(0)?;
            if !seen.contains_key(&next) {
                frontier.push((next, depth + 1));
            }
        }
        drain_rows(&mut rows).await?;
        if seen.len() as u32 >= limit {
            break;
        }
    }

    let mut out: Vec<ImpactNode> = seen.into_values().collect();
    out.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.path.cmp(&b.path)));
    out.truncate(limit as usize);
    Ok(out)
}

async fn resolve_seeds(conn: &Connection, target: &str) -> Result<Vec<String>> {
    let t = target.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    // Exact node id
    {
        let mut rows = conn
            .query("SELECT id FROM cg_nodes WHERE id = ? LIMIT 1", params![t])
            .await?;
        if let Some(row) = rows.next().await? {
            let id = row.get::<String>(0)?;
            drain_rows(&mut rows).await?;
            return Ok(vec![id]);
        }
        drain_rows(&mut rows).await?;
    }
    // Path match
    {
        let mut rows = conn
            .query(
                "SELECT id FROM cg_nodes WHERE path = ? OR path LIKE ? LIMIT 20",
                params![t, format!("%{t}")],
            )
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }
        drain_rows(&mut rows).await?;
        if !ids.is_empty() {
            return Ok(ids);
        }
    }
    // Symbol name
    {
        let mut rows = conn
            .query("SELECT id FROM cg_nodes WHERE name = ? LIMIT 20", params![t])
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }
        drain_rows(&mut rows).await?;
        if !ids.is_empty() {
            return Ok(ids);
        }
    }
    // File node
    Ok(vec![format!("file:{t}")])
}

async fn load_node(conn: &Connection, id: &str) -> Result<Option<ImpactNode>> {
    let mut rows = conn
        .query("SELECT id, path, name, kind FROM cg_nodes WHERE id = ?", params![id])
        .await?;
    let node = if let Some(row) = rows.next().await? {
        Some(ImpactNode {
            id: row.get(0)?,
            path: row.get(1)?,
            name: row.get(2)?,
            kind: row.get(3)?,
            depth: 0,
        })
    } else {
        None
    };
    drain_rows(&mut rows).await?;
    Ok(node)
}
