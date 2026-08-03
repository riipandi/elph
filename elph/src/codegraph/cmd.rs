//! CLI entry for `elph codegraph`.

use anyhow::Result;
use elph_agent::try_block_on;
use floppy::{ScanStats, SearchOptions};

use super::store::open_store;
use crate::cli::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED, S_OK, S_TIP, S_TITLE, S_VALUE, S_WARN};
use crate::cli::CodegraphCommands;
use crate::platform::Paths;

pub fn run(paths: Paths, cmd: &CodegraphCommands) -> Result<()> {
    let sty = CliStyle::auto();

    match cmd {
        CodegraphCommands::Build => {
            let stats = run_scan_with_spinner(&paths, true)?;
            print_scan(&sty, "build", &stats);
        }
        CodegraphCommands::Update => {
            let stats = run_scan_with_spinner(&paths, false)?;
            print_scan(&sty, "update", &stats);
        }
        CodegraphCommands::Status => {
            let store = open_store(&paths, false)?;
            let st = try_block_on(store.status())??;
            let mut out = String::new();
            style::section(&mut out, sty, "Codegraph index");
            style::kv(&mut out, sty, "Files", st.file_count);
            style::kv(&mut out, sty, "Chunks", st.chunk_count);
            style::kv(&mut out, sty, "Nodes", st.node_count);
            style::kv(&mut out, sty, "Edges", st.edge_count);
            if let Some(r) = st.merkle_root {
                style::kv(&mut out, sty, "Merkle root", &r[..16.min(r.len())]);
            }
            if let Some(t) = st.last_indexed_at {
                style::kv(&mut out, sty, "Last indexed", t);
            }
            if let Some(d) = st.root_dir {
                style::kv(&mut out, sty, "Root", d);
            }
            if st.file_count == 0 {
                use std::fmt::Write;
                let _ = writeln!(out);
                style::tip(&mut out, sty, "Index empty — run: elph codegraph build");
            }
            print!("{out}");
        }
        CodegraphCommands::Purge => {
            let store = open_store(&paths, false)?;
            try_block_on(store.purge())??;
            let mut out = String::new();
            style::success(&mut out, sty, "Codegraph index purged.");
            print!("{out}");
        }
        CodegraphCommands::Search { query, limit } => {
            if query.is_empty() {
                anyhow::bail!("usage: elph codegraph search <query>");
            }
            let store = open_store(&paths, true)?;
            let opts = SearchOptions {
                query: query.join(" "),
                limit: *limit,
                refresh_dirty: true,
            };
            let hits = try_block_on(store.search(opts))??;
            let mut out = String::new();
            if hits.is_empty() {
                style::info(&mut out, sty, sty.paint(S_MUTED, "No matches."));
            } else {
                style::section(
                    &mut out,
                    sty,
                    &format!("Search · {} result(s) for \"{}\"", hits.len(), query.join(" ")),
                );
                use std::fmt::Write;
                let _ = writeln!(out);
                for (i, h) in hits.iter().enumerate() {
                    let name = h.name.as_deref().unwrap_or("-");
                    let _ = writeln!(
                        out,
                        "{}  {}  {}:{}-{}  {}",
                        sty.paint(S_ACCENT, format!("{}.", i + 1)),
                        sty.paint(S_BODY, &h.path),
                        sty.paint(S_MUTED, name),
                        sty.paint(S_MUTED, h.start_line),
                        sty.paint(S_MUTED, h.end_line),
                        sty.paint(S_HEADER, &h.kind),
                    );
                    let score_pct = h.score * 100.0;
                    let source_label = match h.source.as_str() {
                        "both" => "keyword + vector",
                        "fts" => "keyword",
                        _ => "vector",
                    };
                    let _ = writeln!(
                        out,
                        "   {}  match {:.0}%  ({})",
                        sty.paint(S_MUTED, "·"),
                        score_pct,
                        sty.paint(S_MUTED, source_label),
                    );
                    for line in h.snippet.lines().take(4) {
                        let _ = writeln!(out, "   {}", sty.paint(S_MUTED, line));
                    }
                    if i + 1 < hits.len() {
                        let _ = writeln!(out);
                    }
                }
            }
            print!("{out}");
        }
        CodegraphCommands::Impact { target, depth, limit } => {
            let store = open_store(&paths, false)?;
            let nodes = try_block_on(store.impact(target, *depth, *limit))??;
            let mut out = String::new();
            if nodes.is_empty() {
                style::info(&mut out, sty, sty.paint(S_MUTED, format!("No impact nodes for \"{target}\".")));
            } else {
                style::section(&mut out, sty, &format!("Impact · {} node(s) for \"{target}\"", nodes.len()));
                use std::fmt::Write;
                let _ = writeln!(out);
                for n in &nodes {
                    let name = n.name.as_deref().unwrap_or("-");
                    let _ = writeln!(
                        out,
                        "  {}  {}  {}  {}",
                        sty.paint(S_ACCENT, format!("d{}", n.depth)),
                        sty.paint(S_BODY, &n.path),
                        sty.paint(S_MUTED, name),
                        sty.paint(S_HEADER, &n.kind),
                    );
                }
            }
            print!("{out}");
        }
    }
    Ok(())
}

fn run_scan_with_spinner(paths: &Paths, full_build: bool) -> Result<ScanStats> {
    use std::sync::Arc;

    use elph_tui::CliSpinner;
    use floppy::{IndexPhase, ProgressFn};

    let spinner = CliSpinner::new(if full_build {
        "Building codegraph index…"
    } else {
        "Updating codegraph index…"
    });
    let spinner_cb = spinner.clone();
    let progress: ProgressFn = Arc::new(move |ev| {
        let msg = match ev.phase {
            IndexPhase::Starting => "Opening store…".into(),
            IndexPhase::Scanning => "Scanning files…".into(),
            IndexPhase::IndexingFile => match &ev.current_path {
                Some(p) => format!("{p}  ({} reindexed · {} seen)", ev.files_indexed, ev.files_walked),
                None => format!("Indexing…  ({} reindexed · {} seen)", ev.files_indexed, ev.files_walked),
            },
            IndexPhase::Finalizing => "Finalizing…".into(),
            IndexPhase::Done => "Done".into(),
        };
        spinner_cb.set_message(msg);
    });

    let store = open_store(paths, true)?;
    let stats = if full_build {
        try_block_on(store.build_with_progress(Some(progress)))??
    } else {
        try_block_on(store.update_with_progress(Some(progress)))??
    };
    spinner.finish_and_clear();
    Ok(stats)
}

fn print_scan(sty: &CliStyle, label: &str, stats: &ScanStats) {
    let mut out = String::new();
    style::section(&mut out, *sty, &format!("Codegraph {label}"));
    style::kv(&mut out, *sty, "Walked", stats.files_walked);
    style::kv(&mut out, *sty, "Skipped", stats.files_skipped);
    style::kv(&mut out, *sty, "Unchanged", stats.files_unchanged);
    style::kv(&mut out, *sty, "Indexed", stats.files_indexed);
    style::kv(&mut out, *sty, "Chunks", stats.chunks_indexed);
    style::kv(&mut out, *sty, "Embedded", stats.chunks_embedded);
    style::kv(&mut out, *sty, "Bytes read", style::fmt_bytes(stats.bytes_read));
    style::kv(&mut out, *sty, "Walk time", style::fmt_duration(stats.walk_ms));
    style::kv(&mut out, *sty, "Reindex time", style::fmt_duration(stats.reindex_ms));
    style::kv(&mut out, *sty, "Finalize time", style::fmt_duration(stats.finalize_ms));
    print!("{out}");
}
