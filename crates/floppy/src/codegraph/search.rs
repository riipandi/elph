//! Hybrid FTS + vector search with RRF merge.

use anyhow::Result;
use std::collections::HashMap;
use turso::{Connection, params};

use super::migrations::fts_available;
use super::types::{ChunkHit, ImpactNode, SearchOptions};
use crate::core::embed::EmbedFn;
use crate::core::fts::sanitize_query;
use crate::core::util::{drain_rows, is_zero, vec_buf};

const RRF_K: f64 = 60.0;

pub async fn hybrid_search(conn: &Connection, embed: &EmbedFn, opts: &SearchOptions) -> Result<Vec<ChunkHit>> {
    let limit = opts.limit.clamp(1, 50) as usize;
    let cand = (limit * 4).max(20);

    let mut ranks: HashMap<i64, (f64, ChunkHit)> = HashMap::new();

    // FTS path (Turso Tantivy). `SELECT * ... WHERE fts_match(cols, ?1)` is
    // routed through Tantivy and returns rows in BM25-descending order, which
    // is exactly the ranking RRF needs — no score column required.
    if fts_available(conn).await.unwrap_or(false) {
        let fts_q = sanitize_query(&opts.query);
        if !fts_q.is_empty() {
            let sql = "SELECT * FROM cg_chunks WHERE fts_match(content, path, name, kind, ?1)".to_string();
            if let Ok(mut rows) = conn.query(&sql, params![fts_q.as_str()]).await {
                let mut i = 0usize;
                while let Some(row) = rows.next().await? {
                    i += 1;
                    if i > cand {
                        break;
                    }
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
                        if h.source == "fts" {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::index::purge_all;
    use crate::codegraph::migrations;
    use crate::core::embed::noop_embedder;
    use turso::Builder;

    async fn seed_conn() -> Connection {
        let db = Builder::new_local(":memory:")
            .experimental_index_method(true)
            .build()
            .await
            .expect("build");
        let conn = db.connect().expect("connect");
        migrations::apply(&conn).await.expect("apply");

        for (path, kind, name, content) in [
            (
                "src/main.rs",
                "function",
                Some("main"),
                "fn main() { println!(\"rust hello\"); }",
            ),
            (
                "src/lib.rs",
                "function",
                Some("borrow"),
                "the borrow checker rules the rust type system",
            ),
            ("src/lib.rs", "struct", None, "milk and cookies are stored in the fridge"),
            (
                "src/util.rs",
                "function",
                Some("parse"),
                "parse arguments with clap and milk the parser",
            ),
            (
                "src/hot.rs",
                "function",
                Some("hot"),
                "rust rust rust rust rust borrow checker hot path",
            ),
        ] {
            conn.execute(
                "INSERT INTO cg_chunks (path, kind, name, start_line, end_line, content, file_hash)
                 VALUES (?, ?, ?, 1, 2, ?, 'abc')",
                params![path, kind, name, content],
            )
            .await
            .expect("insert chunk");
        }
        conn
    }

    #[tokio::test]
    async fn fts_ranks_by_bm25_and_joins_terms() {
        let conn = seed_conn().await;
        let embed = noop_embedder(4);
        let opts = SearchOptions {
            query: "rust".into(),
            limit: 10,
            refresh_dirty: false,
        };

        // BM25 order: the repeated-"rust" chunk outranks single occurrences.
        let hits = hybrid_search(&conn, &embed, &opts).await.expect("search");
        assert_eq!(hits.first().map(|h| h.path.as_str()), Some("src/hot.rs"));
        assert!(hits.iter().all(|h| h.source == "fts"));
        assert!(hits.iter().all(|h| h.score > 0.0));

        // Exact term: "borrow" matches the borrow-checker chunk.
        let hits = hybrid_search(
            &conn,
            &embed,
            &SearchOptions {
                query: "borrow".into(),
                limit: 10,
                refresh_dirty: false,
            },
        )
        .await
        .expect("term search");
        assert!(hits.iter().any(|h| h.name.as_deref() == Some("borrow")));

        // Tantivy 0.26 has no single-token prefix queries: the `*` is ignored
        // and "borr*" degrades to the exact term "borr", which matches nothing.
        let hits = hybrid_search(
            &conn,
            &embed,
            &SearchOptions {
                query: "borr*".into(),
                limit: 10,
                refresh_dirty: false,
            },
        )
        .await
        .expect("prefix search");
        assert!(hits.is_empty(), "Tantivy treats term* as an exact term (no prefix expansion)");

        // AND-joined tokens narrow results to chunks mentioning both.
        let hits = hybrid_search(
            &conn,
            &embed,
            &SearchOptions {
                query: "milk fridge".into(),
                limit: 10,
                refresh_dirty: false,
            },
        )
        .await
        .expect("and search");
        assert!(hits.iter().any(|h| h.snippet.contains("fridge")));

        // Nullable `name` column indexes fine (struct chunk with name IS NULL).
        let hits = hybrid_search(
            &conn,
            &embed,
            &SearchOptions {
                query: "cookies".into(),
                limit: 10,
                refresh_dirty: false,
            },
        )
        .await
        .expect("null name search");
        assert!(hits.iter().any(|h| h.name.is_none()));
    }

    #[tokio::test]
    async fn purge_keeps_fts_available_flag() {
        let conn = seed_conn().await;
        purge_all(&conn).await.expect("purge");
        assert!(migrations::fts_available(&conn).await.expect("fts flag"));

        // After purge, keyword search returns nothing but still routes to FTS.
        let embed = noop_embedder(4);
        let opts = SearchOptions {
            query: "rust".into(),
            limit: 10,
            refresh_dirty: false,
        };
        let hits = hybrid_search(&conn, &embed, &opts).await.expect("search");
        assert!(hits.is_empty());
    }
}
