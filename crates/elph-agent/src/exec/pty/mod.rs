//! PTY allocation and session setup (rustix + tokio).

#[cfg(unix)]
mod sys;

#[cfg(unix)]
pub use sys::{Pts, PtyMaster, PtySize, open_pty};
