//! CLI entry for `elph codegraph`.

use anyhow::Result;
use elph_agent::try_block_on;
use floppy::{ScanStats, SearchOptions};

use super::store::open_store;
use crate::cli::CodegraphCommands;
use crate::cli::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED};
use crate::platform::Paths;

use std::fmt::Write;

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

            // Header
            style::section(&mut out, sty, "Codegraph status");

            let _ = writeln!(out);

            // Key metrics in compact format
            let _ = writeln!(
                out,
                "  {} {}  ({} chunks, {} nodes, {} edges)",
                sty.paint(S_ACCENT, format!("{}", st.file_count)),
                if st.file_count == 1 { "file" } else { "files" },
                st.chunk_count,
                st.node_count,
                st.edge_count
            );

            // Only show additional info if index is not empty
            if st.file_count > 0 {
                // Last indexed time
                if let Some(t) = st.last_indexed_at {
                    let time_str = if t > 0 {
                        // Format as relative time if recent
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                        let diff = now - t;
                        if diff < 60 {
                            format!("{}s ago", diff)
                        } else if diff < 3600 {
                            format!("{}m ago", diff / 60)
                        } else if diff < 86400 {
                            format!("{}h ago", diff / 3600)
                        } else {
                            format!("{}d ago", diff / 86400)
                        }
                    } else {
                        "never".to_string()
                    };
                    let _ = writeln!(out, "  Last indexed: {}", sty.paint(S_BODY, time_str));
                }

                // Root directory
                if let Some(d) = st.root_dir {
                    // Show abbreviated path for brevity
                    let display_path = if d.len() > 35 {
                        format!("…{}", &d[d.len() - 33..])
                    } else {
                        d.clone()
                    };
                    let _ = writeln!(out, "  Root: {}", sty.paint(S_MUTED, display_path));
                }
            } else {
                // Empty state hint
                let _ = writeln!(out);
                style::tip(&mut out, sty, "Index empty — run: elph codegraph build");
            }

            print!("{out}");
        }
        CodegraphCommands::Purge { force } => {
            if !force {
                // Ask for confirmation
                use std::io::{self, Write};
                print!("This will delete the entire codegraph index. Continue? [y/N] ");
                io::stdout().flush().unwrap();

                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();

                let response = input.trim().to_lowercase();
                if response != "y" && response != "yes" {
                    let mut out = String::new();
                    style::info(&mut out, sty, "Purge cancelled.");
                    print!("{out}");
                    return Ok(());
                }
            }

            let store = open_store(&paths, false)?;
            try_block_on(store.purge())??;
            let mut out = String::new();
            style::success(&mut out, sty, "Codegraph index purged.");
            let _ = writeln!(out);
            style::tip(&mut out, sty, "Run: elph codegraph build to recreate the index");
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
                style::section(&mut out, sty, &format!("{} result(s) for \"{}\"", hits.len(), query.join(" ")));

                let _ = writeln!(out);

                for (i, h) in hits.iter().enumerate() {
                    let name = h.name.as_deref().unwrap_or("-");
                    let score_pct = h.score * 100.0;
                    let source_label = match h.source.as_str() {
                        "both" => "both",
                        "fts" => "keyword",
                        _ => "vector",
                    };

                    // Compact result line
                    let _ = writeln!(
                        out,
                        "{}  {}  {}:{}-{}  {}  {}",
                        sty.paint(S_ACCENT, format!("{}.", i + 1)),
                        sty.paint(S_BODY, &h.path),
                        sty.paint(S_MUTED, name),
                        sty.paint(S_MUTED, h.start_line),
                        sty.paint(S_MUTED, h.end_line),
                        sty.paint(S_HEADER, &h.kind),
                        sty.paint(S_MUTED, format!("{:.0}% ({})", score_pct, source_label)),
                    );

                    // Snippet (truncated)
                    for line in h.snippet.lines().take(2) {
                        let line = line.trim();
                        if !line.is_empty() {
                            let _ = writeln!(out, "   {}", sty.paint(S_MUTED, line));
                        }
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
            IndexPhase::Scanning => {
                if ev.files_walked > 0 {
                    format!("Scanning files… ({} seen)", ev.files_walked)
                } else {
                    "Scanning files…".into()
                }
            }
            IndexPhase::IndexingFile => {
                let base = if let Some(p) = &ev.current_path {
                    // Show relative path for brevity
                    let display_path = if p.len() > 40 {
                        format!("…{}", &p[p.len() - 38..])
                    } else {
                        p.clone()
                    };
                    format!("{}  ({} of {} files)", display_path, ev.files_indexed, ev.files_walked)
                } else {
                    format!("Indexing…  ({} of {} files)", ev.files_indexed, ev.files_walked)
                };

                if let (Some(total), Some(estimate)) = (ev.files_to_index, ev.estimated_seconds) {
                    let progress_pct = if total > 0 {
                        ((ev.files_indexed as f64 / total as f64) * 100.0) as u32
                    } else {
                        0
                    };

                    let time_str = if estimate < 60 {
                        format!("{}s", estimate)
                    } else if estimate < 3600 {
                        format!("{}m {}s", estimate / 60, estimate % 60)
                    } else {
                        format!("{}h {}m", estimate / 3600, (estimate % 3600) / 60)
                    };

                    format!("{} · {}% · ~{} remaining", base, progress_pct, time_str)
                } else {
                    base
                }
            }
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

    // Success header with context
    let action = if label == "build" { "built" } else { "updated" };
    style::success(&mut out, *sty, format!("Codegraph {action} successfully"));

    use std::fmt::Write;
    let _ = writeln!(out);

    // Key metrics in a compact format
    let files = stats.files_indexed;
    let chunks = stats.chunks_indexed;
    let _ = writeln!(
        out,
        "  {} {} indexed  ({} chunks)",
        sty.paint(S_ACCENT, format!("{}", files)),
        if files == 1 { "file" } else { "files" },
        chunks
    );

    // GPU status - prominent and clear
    let gpu_status = match stats.gpu_acceleration.as_deref() {
        Some(s) if s.contains("metal") => format!("{} Metal active", sty.paint(S_ACCENT, "✓")),
        Some(s) if s.contains("cuda") => format!("{} CUDA active", sty.paint(S_ACCENT, "✓")),
        Some(s) if s.contains("disabled") => format!("{} CPU only", sty.paint(S_MUTED, "○")),
        Some(other) => format!("{} {}", sty.paint(S_ACCENT, "✓"), other),
        None => format!("{} CPU only", sty.paint(S_MUTED, "○")),
    };
    let _ = writeln!(out, "  GPU: {}", gpu_status);

    // Total time
    let total_ms = stats.walk_ms + stats.reindex_ms + stats.finalize_ms;
    let time_str = if total_ms < 1000 {
        format!("{}ms", total_ms)
    } else {
        format!("{:.1}s", total_ms as f64 / 1000.0)
    };
    let _ = writeln!(out, "  Time: {}", sty.paint(S_BODY, time_str));

    // Additional context for no-op updates
    if stats.files_indexed == 0 && stats.files_unchanged > 0 {
        let _ = writeln!(out);
        style::info(&mut out, *sty, sty.paint(S_MUTED, "No changes detected"));
    }

    print!("{out}");
}
