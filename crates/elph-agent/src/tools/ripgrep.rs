//! Ripgrep (`rg`) subprocess helpers for fast exploration tools.
//!
//! Prefer the system `rg` binary (same strategy as Grok Build) over rebuilding
//! an in-process file index on every call. Callers fall back to fff-search when
//! `rg` is unavailable or fails to start.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Result, anyhow};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Wall-clock timeout for a single `rg` invocation.
const RG_TIMEOUT: Duration = Duration::from_secs(25);
/// Cap stdout so a runaway match set cannot OOM the agent.
const MAX_STDOUT_BYTES: usize = 5_000_000;

/// Resolved path to the `rg` executable (or `None` if not found / unusable).
pub fn rg_binary() -> Option<&'static Path> {
    static RG: OnceLock<Option<PathBuf>> = OnceLock::new();
    RG.get_or_init(resolve_rg).as_deref()
}

fn resolve_rg() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ELPH_RG_PATH") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    // Prefer PATH lookup; do not shell out to `which` (Windows/macOS/Linux).
    which_rg().or_else(|| {
        // Common install locations when PATH is minimal (GUI-launched agents).
        for candidate in ["/opt/homebrew/bin/rg", "/usr/local/bin/rg", "/usr/bin/rg"] {
            let p = PathBuf::from(candidate);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    })
}

fn which_rg() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(if cfg!(windows) { "rg.exe" } else { "rg" });
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Grep output mode mirrored from the agent tool API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgOutputMode {
    Content,
    FilesWithMatches,
    Count,
}

/// Parameters for one `rg` content/files/count search.
#[derive(Debug, Clone)]
pub struct RgGrepArgs {
    pub pattern: String,
    pub paths: Vec<String>,
    pub glob: Option<String>,
    pub file_type: Option<String>,
    pub mode: RgOutputMode,
    pub ignore_case: bool,
    pub literal: bool,
    pub word_regexp: bool,
    pub before_context: usize,
    pub after_context: usize,
    pub max_count_per_file: Option<usize>,
    /// Max match lines / entries (rg --max-count is per-file; we also head the stream).
    pub head_limit: usize,
    pub multiline: bool,
    /// Working directory for relative path display (rg --null etc. not used).
    pub cwd: String,
}

/// Result of a successful `rg` run.
#[derive(Debug, Clone)]
pub struct RgRunResult {
    pub lines: Vec<String>,
    pub limit_reached: bool,
    #[allow(dead_code)]
    pub timed_out: bool,
}

/// Run content search with ripgrep. Returns `None` if `rg` is missing or cannot start.
pub async fn run_rg_grep(args: &RgGrepArgs, signal: Option<&CancellationToken>) -> Result<Option<RgRunResult>> {
    let Some(rg) = rg_binary() else {
        return Ok(None);
    };
    if signal.is_some_and(|t| t.is_cancelled()) {
        return Err(anyhow!("Operation aborted"));
    }

    let mut cmd = Command::new(rg);
    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    // Never color; never follow symlinks into huge trees by default.
    cmd.arg("--color").arg("never");
    cmd.arg("--line-number");
    cmd.arg("--no-heading");
    cmd.arg("--with-filename");
    // Respect .gitignore / .ignore (rg default). Hidden files stay hidden unless glob forces them.

    match args.mode {
        RgOutputMode::Content => {}
        RgOutputMode::FilesWithMatches => {
            cmd.arg("--files-with-matches");
        }
        RgOutputMode::Count => {
            cmd.arg("--count");
        }
    }

    if args.ignore_case {
        cmd.arg("-i");
    }
    if args.literal {
        cmd.arg("-F");
    }
    if args.word_regexp {
        cmd.arg("-w");
    }
    if args.multiline {
        cmd.arg("-U").arg("--multiline-dotall");
    }
    if args.before_context > 0 {
        cmd.arg("-B").arg(args.before_context.to_string());
    }
    if args.after_context > 0 {
        cmd.arg("-A").arg(args.after_context.to_string());
    }
    if let Some(n) = args.max_count_per_file {
        if n > 0 {
            cmd.arg("--max-count").arg(n.to_string());
        }
    }
    if let Some(ref glob) = args.glob {
        cmd.arg("--glob").arg(glob);
    }
    if let Some(ref ty) = args.file_type {
        cmd.arg("--type").arg(ty);
    }

    // Pattern after flags; `--` separates paths that may start with `-`.
    cmd.arg("--regexp").arg(&args.pattern);
    cmd.arg("--");
    if args.paths.is_empty() {
        cmd.arg(".");
    } else {
        for p in &args.paths {
            cmd.arg(p);
        }
    }

    // Prefer relative paths when searching under cwd.
    if !args.cwd.is_empty() {
        cmd.current_dir(&args.cwd);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            log::debug!("rg spawn failed ({err}); falling back to fff-search");
            return Ok(None);
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let read_fut = async {
        let mut out = Vec::new();
        if let Some(mut pipe) = stdout {
            let mut buf = [0u8; 8192];
            loop {
                if out.len() >= MAX_STDOUT_BYTES {
                    break;
                }
                let n = pipe.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                let take = n.min(MAX_STDOUT_BYTES.saturating_sub(out.len()));
                out.extend_from_slice(&buf[..take]);
            }
        }
        let mut err = Vec::new();
        if let Some(pipe) = stderr {
            let _ = pipe.take(64_000).read_to_end(&mut err).await;
        }
        Ok::<_, std::io::Error>((out, err))
    };

    let timed = tokio::time::timeout(RG_TIMEOUT, async {
        if let Some(token) = signal {
            tokio::select! {
                biased;
                _ = token.cancelled() => {
                    let _ = child.start_kill();
                    Err(anyhow!("Operation aborted"))
                }
                res = read_fut => {
                    let (out, err) = res.map_err(|e| anyhow!("rg io: {e}"))?;
                    let status = child.wait().await.map_err(|e| anyhow!("rg wait: {e}"))?;
                    Ok((out, err, status, false))
                }
            }
        } else {
            let (out, err) = read_fut.await.map_err(|e| anyhow!("rg io: {e}"))?;
            let status = child.wait().await.map_err(|e| anyhow!("rg wait: {e}"))?;
            Ok((out, err, status, false))
        }
    })
    .await;

    let (stdout_buf, _stderr_buf, status, timed_out) = match timed {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Ok(Some(RgRunResult {
                lines: vec!["[grep timed out after 25s — narrow path/glob or pattern]".into()],
                limit_reached: false,
                timed_out: true,
            }));
        }
    };

    // rg exit 0 = matches, 1 = no matches, 2 = error.
    let code = status.code().unwrap_or(2);
    if code == 2 {
        // Binary missing mid-flight or bad regex — fall back when empty and no useful stdout.
        if stdout_buf.is_empty() {
            log::debug!("rg exited 2; falling back to fff-search");
            return Ok(None);
        }
    }

    let text = String::from_utf8_lossy(&stdout_buf);
    let mut lines: Vec<String> = text.lines().map(|l| relativize_line(l, &args.cwd)).collect();

    // Drop trailing empty line artifacts.
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    let limit = args.head_limit.max(1);
    let limit_reached = lines.len() > limit;
    if limit_reached {
        lines.truncate(limit);
    }

    // code 1 with empty = no matches — still success.
    let _ = (code, timed_out);
    Ok(Some(RgRunResult {
        lines,
        limit_reached,
        timed_out: false,
    }))
}

/// List files matching a glob via `rg --files -g`.
pub async fn run_rg_files(
    base: &str,
    glob: &str,
    limit: usize,
    signal: Option<&CancellationToken>,
) -> Result<Option<RgRunResult>> {
    let Some(rg) = rg_binary() else {
        return Ok(None);
    };
    if signal.is_some_and(|t| t.is_cancelled()) {
        return Err(anyhow!("Operation aborted"));
    }

    let mut cmd = Command::new(rg);
    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(base);
    cmd.arg("--color").arg("never");
    cmd.arg("--files");
    cmd.arg("--glob").arg(glob);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            log::debug!("rg --files spawn failed ({err})");
            return Ok(None);
        }
    };
    let stdout = child.stdout.take();
    let read = async {
        let mut out = Vec::new();
        if let Some(pipe) = stdout {
            let _ = pipe.take(MAX_STDOUT_BYTES as u64).read_to_end(&mut out).await;
        }
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((out, status))
    };

    let (stdout_buf, status) = match tokio::time::timeout(RG_TIMEOUT, read).await {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => return Ok(None),
        Err(_) => {
            let _ = child.start_kill();
            return Ok(Some(RgRunResult {
                lines: vec!["[find_path timed out]".into()],
                limit_reached: false,
                timed_out: true,
            }));
        }
    };

    if !status.success() && stdout_buf.is_empty() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&stdout_buf);
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.replace('\\', "/"))
        .collect();
    lines.sort();
    let limit_reached = lines.len() > limit;
    if limit_reached {
        lines.truncate(limit);
    }
    Ok(Some(RgRunResult {
        lines,
        limit_reached,
        timed_out: false,
    }))
}

/// Make absolute paths under `cwd` relative for token efficiency.
fn relativize_line(line: &str, cwd: &str) -> String {
    if cwd.is_empty() {
        return line.replace('\\', "/");
    }
    let norm_cwd = cwd.replace('\\', "/").trim_end_matches('/').to_string();
    let line = line.replace('\\', "/");
    // Formats: `path:line:content`, `path:count`, bare `path`.
    let prefix = format!("{norm_cwd}/");
    if let Some(rest) = line.strip_prefix(&prefix) {
        rest.to_string()
    } else if line.starts_with(&norm_cwd) && line[norm_cwd.len()..].starts_with(':') {
        // rare: exact cwd as path component
        line[norm_cwd.len() + 1..].to_string()
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relativize_strips_cwd_prefix() {
        let line = "/tmp/proj/src/main.rs:10:fn main()";
        assert_eq!(relativize_line(line, "/tmp/proj"), "src/main.rs:10:fn main()");
    }

    #[tokio::test]
    async fn rg_grep_basic_when_available() {
        let Some(_) = rg_binary() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn hello() {}\n").unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        let res = run_rg_grep(
            &RgGrepArgs {
                pattern: "hello".into(),
                paths: vec![".".into()],
                glob: Some("*.rs".into()),
                file_type: None,
                mode: RgOutputMode::Content,
                ignore_case: false,
                literal: true,
                word_regexp: false,
                before_context: 0,
                after_context: 0,
                max_count_per_file: None,
                head_limit: 50,
                multiline: false,
                cwd: cwd.clone(),
            },
            None,
        )
        .await
        .unwrap()
        .expect("rg available");
        assert!(res.lines.iter().any(|l| l.contains("hello")));
    }
}
