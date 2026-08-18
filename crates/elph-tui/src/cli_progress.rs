//! Synchronous CLI progress indicators rendered with iocraft (spinner + stepped bar).
//!
//! Non-fullscreen terminal feedback: examples, auth setup, and startup init steps
//! write a single overwritten stderr line.

use std::borrow::Cow;
use std::io::stderr;
use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::thread::{self};
use std::time::Duration;
use std::time::Instant;

use iocraft::prelude::*;

use crate::loader::SpinnerLoader;

const TICK_INTERVAL: Duration = Duration::from_millis(80);
const BAR_WIDTH: usize = 24;

/// Set when the user pressed Ctrl+C / SIGINT during a CLI progress phase.
///
/// The `elph` binary installs the handler (via [`note_interrupt`]); the progress
/// tick threads poll this flag and abort with a clean "Interrupted." message.
/// No other crate needs to install a signal handler — calling [`note_interrupt`]
/// directly also works (e.g. from a TUI key handler).
static INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Record a user interrupt request (Ctrl+C / SIGINT). Async-signal-safe: only
/// stores to an atomic flag, no allocation or locking.
pub fn note_interrupt() {
    INTERRUPT_REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether the user requested an interrupt during a progress phase.
pub fn interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::SeqCst)
}

/// Whether animated CLI progress should render (TTY stderr, color allowed, not in tests).
pub fn progress_enabled(quiet_env: Option<&'static str>) -> bool {
    if cfg!(test) {
        return false;
    }
    if quiet_env.is_some_and(|name| std::env::var_os(name).is_some()) {
        return false;
    }
    if std::env::var("NO_COLOR").as_deref() == Ok("true") {
        return false;
    }
    stderr().is_terminal()
}

struct SpinnerLineState {
    message: String,
    loader: SpinnerLoader,
    started: Instant,
    finished: bool,
    enabled: bool,
}

struct SpinnerInner {
    state: Mutex<SpinnerLineState>,
    tick_thread: Mutex<Option<JoinHandle<()>>>,
}

/// Braille spinner for CLI examples and short-lived operations.
#[derive(Clone)]
pub struct CliSpinner {
    inner: Arc<SpinnerInner>,
}

impl CliSpinner {
    /// Animated spinner on stderr, or a quiet fallback when progress is disabled.
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        if !progress_enabled(None) {
            eprintln!("{message}");
            return Self::disabled();
        }

        let inner = Arc::new(SpinnerInner {
            state: Mutex::new(SpinnerLineState {
                message,
                loader: SpinnerLoader::new(),
                started: Instant::now(),
                finished: false,
                enabled: true,
            }),
            tick_thread: Mutex::new(None),
        });

        start_tick_thread(&inner);

        {
            let guard = inner.state.lock().expect("spinner lock");
            let line = render_spinner_line(guard.loader.glyph(), &guard.message, guard.started);
            write_overwrite_line(&line);
        }

        Self { inner }
    }

    /// No-op spinner returned when progress output is disabled.
    pub fn disabled() -> Self {
        Self {
            inner: Arc::new(SpinnerInner {
                state: Mutex::new(SpinnerLineState {
                    message: String::new(),
                    loader: SpinnerLoader::new(),
                    started: Instant::now(),
                    finished: true,
                    enabled: false,
                }),
                tick_thread: Mutex::new(None),
            }),
        }
    }

    pub fn set_message(&self, message: impl Into<String>) {
        let mut guard = self.inner.state.lock().expect("spinner lock");
        if !guard.enabled || guard.finished {
            return;
        }
        guard.message = message.into();
        let line = render_spinner_line(guard.loader.glyph(), &guard.message, guard.started);
        write_overwrite_line(&line);
    }

    pub fn finish_and_clear(&self) {
        self.finish_inner(/*newline*/ false);
    }

    /// Stop the spinner, clear the line, and advance the cursor (so the next
    /// stderr/stdout write cannot overwrite residual spinner glyphs).
    pub fn finish_and_clear_with_newline(&self) {
        self.finish_inner(/*newline*/ true);
    }

    fn finish_inner(&self, newline: bool) {
        {
            let mut guard = self.inner.state.lock().expect("spinner lock");
            if guard.finished {
                // Already stopped — still ensure a clean line if we were enabled.
                if guard.enabled {
                    clear_line();
                    if newline {
                        let _ = writeln!(stderr().lock());
                    }
                }
                return;
            }
            guard.finished = true;
            if guard.enabled {
                clear_line();
                if newline {
                    let _ = writeln!(stderr().lock());
                }
            }
        }
        // Do NOT hold the state lock across `join`: the tick thread must acquire
        // it to observe `finished` and exit, so holding it here deadlocks (tick
        // blocks on the lock while we block on the join).
        if let Some(handle) = self.inner.tick_thread.lock().expect("spinner tick lock").take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CliSpinner {
    fn drop(&mut self) {
        // Only the last clone is responsible for teardown. Earlier clones just
        // release their Arc — finish_and_clear must be called explicitly while
        // clones may still be alive (e.g. event tasks).
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        // Best-effort: clear residual spinner if the owner forgot finish_and_clear.
        self.finish_inner(/*newline*/ false);
    }
}

/// Stepped init progress bar (spinner + bar + `pos/len`) for startup sequences.
pub struct CliProgress {
    state: Arc<Mutex<ProgressLineState>>,
    tick_thread: Option<JoinHandle<()>>,
}

struct ProgressLineState {
    message: Cow<'static, str>,
    loader: SpinnerLoader,
    started: Instant,
    pos: u64,
    len: u64,
    finished: bool,
    enabled: bool,
}

impl CliProgress {
    pub fn new(steps: u64) -> Self {
        Self::build(steps, progress_enabled(None))
    }

    pub fn with_quiet_env(mut self, env: &'static str) -> Self {
        if !progress_enabled(Some(env)) {
            self.disable();
        }
        self
    }

    fn build(steps: u64, enabled: bool) -> Self {
        let state = Arc::new(Mutex::new(ProgressLineState {
            message: Cow::Borrowed(""),
            loader: SpinnerLoader::new(),
            started: Instant::now(),
            pos: 0,
            len: steps,
            finished: !enabled,
            enabled,
        }));

        let tick_thread = if enabled {
            let tick_state = Arc::clone(&state);
            Some(thread::spawn(move || {
                loop {
                    {
                        let mut guard = tick_state.lock().expect("progress lock");
                        if guard.finished {
                            break;
                        }
                        guard.loader.tick();
                        let line = render_progress_line(&guard);
                        write_overwrite_line(&line);
                    }
                    if interrupt_requested() {
                        abort_on_interrupt();
                    }
                    thread::sleep(TICK_INTERVAL);
                }
            }))
        } else {
            None
        };

        Self { state, tick_thread }
    }

    fn disable(&mut self) {
        {
            let mut guard = self.state.lock().expect("progress lock");
            guard.enabled = false;
            guard.finished = true;
        }
        if let Some(handle) = self.tick_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn advance(&self, message: impl Into<Cow<'static, str>>) {
        let mut guard = self.state.lock().expect("progress lock");
        if !guard.enabled {
            return;
        }
        guard.pos = guard.pos.saturating_add(1);
        guard.message = message.into();
        let line = render_progress_line(&guard);
        write_overwrite_line(&line);
    }

    pub fn finish(&self) {
        let mut guard = self.state.lock().expect("progress lock");
        if !guard.enabled {
            return;
        }
        guard.finished = true;
        clear_line();
    }
}

impl Drop for CliProgress {
    fn drop(&mut self) {
        if let Some(handle) = self.tick_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Convenience alias matching the old example helper name.
pub fn progress_spinner(message: &str) -> CliSpinner {
    CliSpinner::new(message)
}

/// Spawn the animated tick thread that repaints the spinner line every
/// [`TICK_INTERVAL`] until the shared state flips to `finished`.
///
/// The tick thread must acquire the state lock to observe `finished`, so callers
/// that join it (see [`CliSpinner::finish_and_clear`] / `Drop`) must never hold
/// the state lock across the join.
fn start_tick_thread(inner: &Arc<SpinnerInner>) {
    let tick_inner = Arc::clone(inner);
    let handle = thread::spawn(move || {
        loop {
            {
                let mut guard = tick_inner.state.lock().expect("spinner lock");
                if guard.finished {
                    break;
                }
                guard.loader.tick();
                let line = render_spinner_line(guard.loader.glyph(), &guard.message, guard.started);
                write_overwrite_line(&line);
            }
            if interrupt_requested() {
                abort_on_interrupt();
            }
            thread::sleep(TICK_INTERVAL);
        }
    });
    *inner.tick_thread.lock().expect("spinner tick lock") = Some(handle);
}

/// Abort the process with a clean message when the user pressed Ctrl+C during a
/// CLI progress phase. Runs on a progress tick thread with no locks held.
///
/// Exiting here mirrors the default SIGINT behavior (immediate termination) but
/// leaves a visible "Interrupted." message and the conventional 130 exit code.
/// The main thread is typically blocked in blocking I/O (DB, download) and
/// cannot observe the flag itself, which is why the tick thread owns the abort.
fn abort_on_interrupt() -> ! {
    log::warn!("cli progress interrupted");
    clear_line();
    eprintln!("Interrupted.");
    std::process::exit(130);
}

/// Test-only: build an animated spinner regardless of TTY so the tick-thread
/// spawn + join paths are exercised under `#[cfg(test)]` (where
/// [`progress_enabled`] always reports disabled).
#[cfg(test)]
impl CliSpinner {
    fn new_enabled_for_test(message: &str) -> Self {
        let inner = Arc::new(SpinnerInner {
            state: Mutex::new(SpinnerLineState {
                message: message.to_string(),
                loader: SpinnerLoader::new(),
                started: Instant::now(),
                finished: false,
                enabled: true,
            }),
            tick_thread: Mutex::new(None),
        });
        start_tick_thread(&inner);
        Self { inner }
    }
}

fn render_spinner_line(glyph: &str, message: &str, started: Instant) -> String {
    let mut el = element! {
        View(flex_direction: FlexDirection::Row, align_items: AlignItems::Center) {
            Text(color: Color::Green, wrap: TextWrap::NoWrap, content: glyph.to_string())
            Text(color: Color::Cyan, wrap: TextWrap::NoWrap, content: format!(" {message}"))
            Text(color: Color::DarkGrey, wrap: TextWrap::NoWrap, content: format!(" · {}", format_elapsed(started.elapsed())))
        }
    };
    trim_rendered_line(el.to_string())
}

fn render_progress_line(state: &ProgressLineState) -> String {
    let glyph = state.loader.glyph();
    let (filled, head, empty) = format_bar(state.pos, state.len, BAR_WIDTH);

    let mut el = element! {
        View(flex_direction: FlexDirection::Row, align_items: AlignItems::Center) {
            Text(color: Color::Green, wrap: TextWrap::NoWrap, content: glyph.to_string())
            Text(color: Color::Cyan, wrap: TextWrap::NoWrap, content: format!(" {} ", state.message))
            Text(color: Color::Cyan, wrap: TextWrap::NoWrap, content: "[".to_string())
            Text(color: Color::Cyan, wrap: TextWrap::NoWrap, content: filled)
            Text(color: Color::Blue, wrap: TextWrap::NoWrap, content: head)
            Text(color: Color::Blue, wrap: TextWrap::NoWrap, content: empty)
            Text(
                color: Color::Cyan,
                wrap: TextWrap::NoWrap,
                content: format!("] {}/{}", state.pos, state.len),
            )
            Text(
                color: Color::DarkGrey,
                wrap: TextWrap::NoWrap,
                content: format!(" · {}", format_elapsed(state.started.elapsed())),
            )
        }
    };
    trim_rendered_line(el.to_string())
}

/// Compact human-readable elapsed time for progress lines: `3s`, `1m 05s`.
fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn format_bar(pos: u64, len: u64, width: usize) -> (String, String, String) {
    if len == 0 {
        return (String::new(), String::new(), "─".repeat(width));
    }
    if pos >= len {
        return ("━".repeat(width), String::new(), String::new());
    }

    let mut solid = ((pos as usize) * width / len as usize).min(width);
    if pos > 0 && solid == 0 {
        solid = 1;
    }

    let with_head = solid.min(width.saturating_sub(1));
    let head = if with_head < width {
        "╸".to_string()
    } else {
        String::new()
    };
    let empty = width.saturating_sub(with_head + head.chars().count());

    ("━".repeat(with_head), head, "─".repeat(empty))
}

fn trim_rendered_line(mut line: String) -> String {
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    line
}

fn write_overwrite_line(line: &str) {
    let mut out = stderr().lock();
    let _ = write!(out, "\r{line}\x1b[K");
    let _ = out.flush();
}

fn clear_line() {
    let mut out = stderr().lock();
    let _ = write!(out, "\r\x1b[K");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bar_empty() {
        let (filled, head, empty) = format_bar(0, 5, 8);
        assert!(filled.is_empty());
        assert_eq!(head, "╸");
        assert_eq!(empty.chars().count(), 7);
    }

    #[test]
    fn format_bar_complete() {
        let (filled, head, empty) = format_bar(5, 5, 8);
        assert_eq!(filled.chars().count() + head.chars().count() + empty.chars().count(), 8);
        assert!(head.is_empty());
        assert!(empty.is_empty());
    }

    #[test]
    fn disabled_spinner_finish_is_noop() {
        let spinner = CliSpinner::disabled();
        spinner.finish_and_clear();
    }

    #[test]
    fn finish_and_clear_joins_tick_thread() {
        // Regression: finish_and_clear used to hold the state lock while joining
        // the tick thread, which needs the same lock to observe `finished` — a
        // guaranteed deadlock with an animated spinner ("stuck after indexing").
        let spinner = CliSpinner::new_enabled_for_test("regression");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            spinner.finish_and_clear();
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(1))
            .expect("finish_and_clear deadlocked: tick thread never joined");
    }

    #[test]
    fn disabled_progress_finish_is_noop() {
        let progress = CliProgress::new(3);
        progress.advance("step");
        progress.finish();
    }

    #[test]
    fn render_spinner_line_has_message_and_elapsed() {
        let line = render_spinner_line("⠋", "Loading", Instant::now());
        assert!(line.contains("Loading"));
        assert!(line.contains("·"));
    }

    #[test]
    fn render_progress_line_has_counts() {
        let state = ProgressLineState {
            message: Cow::Borrowed("init"),
            loader: SpinnerLoader::new(),
            started: Instant::now(),
            pos: 2,
            len: 5,
            finished: false,
            enabled: true,
        };
        let line = render_progress_line(&state);
        assert!(line.contains("2/5"));
        assert!(line.contains("init"));
        assert!(line.contains("·"));
    }

    #[test]
    fn format_elapsed_cases() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1m 00s");
        assert_eq!(format_elapsed(Duration::from_secs(125)), "2m 05s");
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "1h 01m");
    }

    #[test]
    fn interrupt_flag_toggles() {
        assert!(!interrupt_requested());
        note_interrupt();
        assert!(interrupt_requested());
        // Reset for other tests in the same process (nextest isolates by default,
        // but keep the helper idempotent-safe for plain `cargo test`).
        INTERRUPT_REQUESTED.store(false, Ordering::SeqCst);
        assert!(!interrupt_requested());
    }
}
