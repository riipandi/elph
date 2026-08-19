//! PTY allocation and session setup (rustix + tokio).

/// Terminal dimensions for a newly allocated PTY.
///
/// Available on every platform so callers can express a requested size without
/// gating on `#[cfg(unix)]`; only the actual PTY allocation (`open_pty`) is Unix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

impl PtySize {
    pub const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

#[cfg(unix)]
impl From<PtySize> for rustix::termios::Winsize {
    fn from(size: PtySize) -> Self {
        rustix::termios::Winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

#[cfg(unix)]
mod sys;

#[cfg(unix)]
pub use sys::{Pts, PtyMaster, open_pty};
