//! Pre-TUI interactive offer to index a project codebase when codegraph is enabled.

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;

use anstyle::{AnsiColor, Color, Style};
use anyhow::Result;
use elph_agent::try_block_on;
use elph_tui::CliSpinner;
use floppy::{IndexPhase, ProgressFn, ScanStats};
use inquire::Select;

use super::store::open_store;
use crate::platform::{Paths, Settings};

const DECLINED_MARKER: &str = "codegraph_index_declined";

/// Run first-access codegraph index offer (interactive CLI, before TUI).
///
/// No-op when: non-TTY, `codegraph.enabled` is false, user previously declined,
/// or the project already has an index.
pub fn maybe_offer_index(paths: &Paths) -> Result<()> {
    if !should_prompt_env() {
        return Ok(());
    }

    let first_access = !paths.project_elph_dir().exists();
    crate::platform::ensure_project(paths)?;

    let settings = Settings::load(paths).unwrap_or_else(|_| Settings::defaults());
    if !settings.codegraph.enabled {
        return Ok(());
    }

    if declined_marker_path(paths).exists() {
        return Ok(());
    }

    // Avoid prompting when an index already exists.
    if let Ok(store) = open_store(paths, false)
        && let Ok(status) = try_block_on(store.status())?
        && status.file_count > 0
    {
        return Ok(());
    }

    let project = paths.project_dir();
    let display_path = display_project_path(project);

    print_offer(&display_path, first_access);

    let choice = prompt_choice()?;
    match choice {
        IndexChoice::Yes => {
            run_index_with_progress(paths)?;
        }
        IndexChoice::Skip => {
            write_declined_marker(paths)?;
            let sty = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
            println!(
                "{}Skipped. Index anytime with: elph codegraph build{}",
                sty.render(),
                sty.render_reset()
            );
            println!();
        }
    }
    Ok(())
}

fn should_prompt_env() -> bool {
    if cfg!(test) {
        return false;
    }
    if std::env::var_os("ELPH_QUIET").is_some() {
        return false;
    }
    if std::env::var_os("CI").is_some() {
        return false;
    }
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

#[derive(Clone, Copy)]
enum IndexChoice {
    Yes,
    Skip,
}

impl std::fmt::Display for IndexChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Yes => write!(f, "Yes!"),
            Self::Skip => write!(f, "Skip"),
        }
    }
}

fn prompt_choice() -> Result<IndexChoice> {
    let options = vec![IndexChoice::Yes, IndexChoice::Skip];
    let ans = Select::new("Index this codebase now?", options)
        .with_page_size(2)
        .with_help_message("↑↓ navigate · Enter confirm · Esc = Skip")
        .prompt_skippable()
        .ok()
        .flatten();
    Ok(ans.unwrap_or(IndexChoice::Skip))
}

fn print_offer(project_path: &str, first_access: bool) {
    let title = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Cyan)));
    let muted = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
    let path_s = Style::new().bold();
    let rule = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));

    println!();
    if first_access {
        println!(
            "{}Would you like to initialize this project?{}",
            title.render(),
            title.render_reset()
        );
    } else {
        println!(
            "{}Would you like to index this codebase?{}",
            title.render(),
            title.render_reset()
        );
    }
    println!("{}{}{}", rule.render(), "─".repeat(40), rule.render_reset());
    println!();
    println!("  {}{}{}", path_s.render(), project_path, path_s.render_reset());
    println!();
    println!("When Elph indexes your codebase it examines source files");
    println!("and stores a semantic index in .elph/store.db for fast");
    println!("code search (code_search / code_impact agent tools).");
    println!();
    println!(
        "{}You can also index anytime with: elph codegraph build{}",
        muted.render(),
        muted.render_reset()
    );
    println!();
    let _ = std::io::stdout().flush();
}

fn display_project_path(project: &Path) -> String {
    let path = project.display().to_string();
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path
}

fn declined_marker_path(paths: &Paths) -> std::path::PathBuf {
    paths.project_elph_dir().join(DECLINED_MARKER)
}

fn write_declined_marker(paths: &Paths) -> Result<()> {
    let path = declined_marker_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        b"# User skipped the startup codegraph index prompt.\n# Delete this file to be asked again.\n",
    )?;
    Ok(())
}

fn clear_declined_marker(paths: &Paths) {
    let _ = std::fs::remove_file(declined_marker_path(paths));
}

fn run_index_with_progress(paths: &Paths) -> Result<()> {
    let spinner = CliSpinner::new("Preparing embedder…");
    let spinner_cb = spinner.clone();
    let progress: ProgressFn = Arc::new(move |ev| {
        let msg = match ev.phase {
            IndexPhase::Starting => "Opening index store…".to_string(),
            IndexPhase::Scanning => "Scanning project files…".to_string(),
            IndexPhase::IndexingFile => {
                if let Some(p) = &ev.current_path {
                    let short = truncate_path(p, 48);
                    format!(
                        "Indexing {short}  ({indexed} reindexed · {walked} seen)",
                        indexed = ev.files_indexed,
                        walked = ev.files_walked
                    )
                } else {
                    format!("Indexing…  ({} reindexed · {} seen)", ev.files_indexed, ev.files_walked)
                }
            }
            IndexPhase::Finalizing => "Finalizing Merkle fingerprint…".to_string(),
            IndexPhase::Done => "Index complete".to_string(),
        };
        spinner_cb.set_message(msg);
    });

    let store = open_store(paths, true)?;
    let stats = try_block_on(store.build_with_progress(Some(progress)))??;
    spinner.finish_and_clear();
    clear_declined_marker(paths);
    print_index_done(&stats);
    Ok(())
}

fn truncate_path(path: &str, max: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max {
        return path.to_string();
    }
    let keep = max.saturating_sub(1);
    let suffix: String = chars[chars.len() - keep..].iter().collect();
    format!("…{suffix}")
}

fn print_index_done(stats: &ScanStats) {
    let ok = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
    let muted = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
    println!(
        "{}✓ Codebase indexed{}  {}{} files · {} chunks · {} embedded{}",
        ok.render(),
        ok.render_reset(),
        muted.render(),
        stats.files_indexed.max(stats.files_unchanged),
        stats.chunks_indexed,
        stats.chunks_embedded,
        muted.render_reset()
    );
    println!(
        "{}  walk {} ms · reindex {} ms · finalize {} ms{}",
        muted.render(),
        stats.walk_ms,
        stats.reindex_ms,
        stats.finalize_ms,
        muted.render_reset()
    );
    println!();
}

/// Unit-test helpers (markers only).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;

    #[test]
    fn declined_marker_roundtrip() {
        let tmp = tempfile::tempdir().expect("tmp");
        let paths = Paths::from_dirs(tmp.path().join("cfg"), tmp.path().join("data"), tmp.path().join("proj"));
        crate::platform::ensure_project(&paths).expect("ensure");
        assert!(!declined_marker_path(&paths).exists());
        write_declined_marker(&paths).expect("write");
        assert!(declined_marker_path(&paths).exists());
        clear_declined_marker(&paths);
        assert!(!declined_marker_path(&paths).exists());
    }

    #[test]
    fn display_path_prefixes_home() {
        if let Ok(home) = std::env::var("HOME") {
            let p = Path::new(&home).join("work/demo");
            let d = display_project_path(&p);
            assert!(d.starts_with("~/"), "{d}");
        }
    }
}
