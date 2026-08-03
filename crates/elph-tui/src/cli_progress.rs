//! Synchronous CLI progress indicators rendered with iocraft (spinner + stepped bar).
//!
//! Non-fullscreen terminal feedback: examples, auth setup, and startup init steps
//! write a single overwritten stderr line.

use std::borrow::Cow;
use std::io::stderr;
use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::thread::{self};
use std::time::Duration;

use iocraft::prelude::*;

use crate::loader::SpinnerLoader;

const TICK_INTERVAL: Duration = Duration::from_millis(80);
const BAR_WIDTH: usize = 24;

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
                finished: false,
                enabled: true,
            }),
            tick_thread: Mutex::new(None),
        });

        start_tick_thread(&inner);

        {
            let guard = inner.state.lock().expect("spinner lock");
            let line = render_spinner_line(guard.loader.glyph(), &guard.message);
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
        let line = render_spinner_line(guard.loader.glyph(), &guard.message);
        write_overwrite_line(&line);
    }

    pub fn finish_and_clear(&self) {
        {
            let mut guard = self.inner.state.lock().expect("spinner lock");
            if !guard.enabled {
                return;
            }
            guard.finished = true;
            clear_line();
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
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        {
            let mut guard = self.inner.state.lock().expect("spinner lock");
            guard.finished = true;
        }
        if let Some(handle) = self.inner.tick_thread.lock().expect("spinner tick lock").take() {
            let _ = handle.join();
        }
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
            pos: 0,
            len: steps,
            finished: !enabled,
            enabled,
        }));

        let tick_thread = if enabled {
            let tick_state = Arc::clone(&state);
            Some(thread::spawn(move || {
                while !tick_state.lock().expect("progress lock").finished {
                    {
                        let mut guard = tick_state.lock().expect("progress lock");
                        if guard.finished {
                            break;
                        }
                        guard.loader.tick();
                        let line = render_progress_line(&guard);
                        write_overwrite_line(&line);
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
                let line = render_spinner_line(guard.loader.glyph(), &guard.message);
                write_overwrite_line(&line);
            }
            thread::sleep(TICK_INTERVAL);
        }
    });
    *inner.tick_thread.lock().expect("spinner tick lock") = Some(handle);
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
                finished: false,
                enabled: true,
            }),
            tick_thread: Mutex::new(None),
        });
        start_tick_thread(&inner);
        Self { inner }
    }
}

fn render_spinner_line(glyph: &str, message: &str) -> String {
    let mut el = element! {
        View(flex_direction: FlexDirection::Row, align_items: AlignItems::Center) {
            Text(color: Color::Green, wrap: TextWrap::NoWrap, content: glyph.to_string())
            Text(color: Color::Cyan, wrap: TextWrap::NoWrap, content: format!(" {message}"))
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
        }
    };
    trim_rendered_line(el.to_string())
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
    fn render_spinner_line_has_message() {
        let line = render_spinner_line("⠋", "Loading");
        assert!(line.contains("Loading"));
    }

    #[test]
    fn render_progress_line_has_counts() {
        let state = ProgressLineState {
            message: Cow::Borrowed("init"),
            loader: SpinnerLoader::new(),
            pos: 2,
            len: 5,
            finished: false,
            enabled: true,
        };
        let line = render_progress_line(&state);
        assert!(line.contains("2/5"));
        assert!(line.contains("init"));
    }
}
