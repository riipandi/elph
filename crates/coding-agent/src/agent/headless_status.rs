//! Lightweight wait indicator for `elph run` (stderr only).
//!
//! Intentionally **not** based on iocraft: headless runs under a CLI `block_on`
//! path where full-element layout each tick is unnecessary and has been observed
//! to leave a frozen first frame. This module writes a simple braille + message
//! + elapsed line with `\r` overwrite — same idea as other CLI tools.

use std::io::{IsTerminal, Write, stderr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(80);
const GLYPHS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Animated wait line on stderr for headless runs.
pub struct HeadlessStatus {
    inner: Arc<Inner>,
    /// When false, this handle is a clone and must not join the tick thread.
    owner: bool,
}

struct Inner {
    message: Mutex<String>,
    finished: AtomicBool,
    enabled: bool,
    started: Instant,
    tick: Mutex<Option<JoinHandle<()>>>,
    frame: Mutex<usize>,
}

impl HeadlessStatus {
    pub fn start(message: impl Into<String>) -> Self {
        let message = message.into();
        let enabled = status_enabled();
        if !enabled {
            // One quiet line so non-TTY still shows something once.
            let _ = writeln!(stderr(), "{message}");
            return Self {
                inner: Arc::new(Inner {
                    message: Mutex::new(message),
                    finished: AtomicBool::new(true),
                    enabled: false,
                    started: Instant::now(),
                    tick: Mutex::new(None),
                    frame: Mutex::new(0),
                }),
                owner: true,
            };
        }

        let inner = Arc::new(Inner {
            message: Mutex::new(message),
            finished: AtomicBool::new(false),
            enabled: true,
            started: Instant::now(),
            tick: Mutex::new(None),
            frame: Mutex::new(0),
        });

        // Paint immediately so the user sees feedback before the first tick.
        paint_line(&inner);

        let tick_inner = Arc::clone(&inner);
        let handle = thread::Builder::new()
            .name("elph-run-status".into())
            .spawn(move || {
                while !tick_inner.finished.load(Ordering::Relaxed) {
                    {
                        let mut frame = tick_inner.frame.lock().expect("frame");
                        *frame = frame.wrapping_add(1);
                    }
                    paint_line(&tick_inner);
                    if elph_tui::interrupt_requested() {
                        clear_line();
                        let _ = writeln!(stderr(), "Interrupted.");
                        std::process::exit(130);
                    }
                    thread::sleep(TICK);
                }
            })
            .expect("spawn elph-run-status");

        *inner.tick.lock().expect("tick") = Some(handle);

        Self {
            inner,
            owner: true,
        }
    }

    pub fn handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            owner: false,
        }
    }

    pub fn set(&self, message: impl Into<String>) {
        if self.inner.finished.load(Ordering::Relaxed) || !self.inner.enabled {
            return;
        }
        *self.inner.message.lock().expect("message") = message.into();
        paint_line(&self.inner);
    }

    /// Stop animation, clear the line, leave a newline for subsequent output.
    pub fn finish(&self) {
        if self
            .inner
            .finished
            .swap(true, Ordering::SeqCst)
        {
            // Already finished — still ensure a clean line once for the owner.
            if self.owner && self.inner.enabled {
                clear_line();
            }
            self.join_tick();
            return;
        }
        if self.inner.enabled {
            clear_line();
            // Advance past the cleared spinner row so stdout/footer never share it.
            let _ = writeln!(stderr());
        }
        self.join_tick();
    }

    /// Stop without forcing an extra blank line (stream formats that already write).
    pub fn finish_quiet(&self) {
        if self.inner.finished.swap(true, Ordering::SeqCst) {
            self.join_tick();
            return;
        }
        if self.inner.enabled {
            clear_line();
        }
        self.join_tick();
    }

    pub fn is_finished(&self) -> bool {
        self.inner.finished.load(Ordering::Relaxed)
    }

    fn join_tick(&self) {
        if !self.owner {
            return;
        }
        if let Some(handle) = self.inner.tick.lock().expect("tick").take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HeadlessStatus {
    fn drop(&mut self) {
        if self.owner {
            self.finish();
        }
    }
}

fn status_enabled() -> bool {
    if cfg!(test) {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    stderr().is_terminal()
}

fn paint_line(inner: &Inner) {
    if !inner.enabled || inner.finished.load(Ordering::Relaxed) {
        return;
    }
    let msg = inner.message.lock().expect("message").clone();
    let frame = *inner.frame.lock().expect("frame");
    let glyph = GLYPHS[frame % GLYPHS.len()];
    let elapsed = format_elapsed(inner.started.elapsed());
    // Dim message slightly with bright-black if color allowed (simple ANSI, no iocraft).
    let line = format!("\r\x1b[32m{glyph}\x1b[0m \x1b[36m{msg}\x1b[0m \x1b[90m· {elapsed}\x1b[0m\x1b[K");
    let mut out = stderr().lock();
    let _ = write!(out, "{line}");
    let _ = out.flush();
}

fn clear_line() {
    let mut out = stderr().lock();
    let _ = write!(out, "\r\x1b[K");
    let _ = out.flush();
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_finish_is_noop() {
        let s = HeadlessStatus::start("test");
        // In tests status_enabled is false → already finished.
        assert!(s.is_finished());
        s.finish();
    }

    #[test]
    fn format_elapsed_basic() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(90)), "1m 30s");
    }
}
