//! CLI entry for `elph codegraph`.

use anyhow::Result;
use elph_agent::try_block_on;
use floppy::{ScanStats, SearchOptions};

use super::store::open_store;
use crate::cli::CodegraphCommands;
use crate::platform::Paths;

pub fn run(paths: Paths, cmd: &CodegraphCommands) -> Result<()> {
    match cmd {
        CodegraphCommands::Build => {
            let stats = run_scan_with_spinner(&paths, true)?;
            print_scan("build", &stats);
        }
        CodegraphCommands::Update => {
            let stats = run_scan_with_spinner(&paths, false)?;
            print_scan("update", &stats);
        }
        CodegraphCommands::Status => {
            let store = open_store(&paths, false)?;
            let st = try_block_on(store.status())??;
            println!("codegraph status");
            println!("  files:    {}", st.file_count);
            println!("  chunks:   {}", st.chunk_count);
            println!("  nodes:    {}", st.node_count);
            println!("  edges:    {}", st.edge_count);
            if let Some(r) = st.merkle_root {
                println!("  merkle:   {r}");
            } else {
                println!("  merkle:   (not built)");
            }
            if let Some(t) = st.last_indexed_at {
                println!("  indexed:  {t}");
            }
            if let Some(d) = st.root_dir {
                println!("  root:     {d}");
            }
            if st.file_count == 0 {
                println!();
                println!("Index empty — run: elph codegraph build");
            }
        }
        CodegraphCommands::Purge => {
            let store = open_store(&paths, false)?;
            try_block_on(store.purge())??;
            println!("codegraph index purged");
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
            if hits.is_empty() {
                println!("No matches.");
            } else {
                for (i, h) in hits.iter().enumerate() {
                    let name = h.name.as_deref().unwrap_or("-");
                    println!(
                        "{}. {} {}:{}-{}  [{}] score={:.4} ({})",
                        i + 1,
                        h.path,
                        name,
                        h.start_line,
                        h.end_line,
                        h.kind,
                        h.score,
                        h.source
                    );
                    for line in h.snippet.lines().take(4) {
                        println!("    {line}");
                    }
                }
            }
        }
        CodegraphCommands::Impact { target, depth, limit } => {
            let store = open_store(&paths, false)?;
            let nodes = try_block_on(store.impact(target, *depth, *limit))??;
            if nodes.is_empty() {
                println!("No impact nodes for {target:?}");
            } else {
                for n in nodes {
                    let name = n.name.as_deref().unwrap_or("-");
                    println!("d{}  {}  {}  {} ({})", n.depth, n.path, name, n.kind, n.id);
                }
            }
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

fn print_scan(label: &str, stats: &ScanStats) {
    println!("codegraph {label}");
    println!("  walked:     {}", stats.files_walked);
    println!("  skipped:    {}", stats.files_skipped);
    println!("  unchanged:  {}", stats.files_unchanged);
    println!("  indexed:    {}", stats.files_indexed);
    println!("  chunks:     {}", stats.chunks_indexed);
    println!("  embedded:   {}", stats.chunks_embedded);
    println!("  bytes:      {}", stats.bytes_read);
    println!("  walk:       {} ms", stats.walk_ms);
    println!("  reindex:    {} ms", stats.reindex_ms);
    println!("  finalize:   {} ms", stats.finalize_ms);
}
