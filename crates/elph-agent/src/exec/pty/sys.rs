//! Unix PTY helpers via rustix.

use std::ffi::OsStr;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

use super::super::error::{ExecError, ExecErrorCode, Result};

/// Terminal dimensions for a newly allocated PTY.
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

impl From<PtySize> for Winsize {
    fn from(size: PtySize) -> Self {
        Winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

/// Master end of a pseudo-terminal pair.
pub struct PtyMaster(OwnedFd);

impl PtyMaster {
    pub fn set_term_size(&self, size: PtySize) -> Result<()> {
        tcsetwinsize(self.as_fd(), Winsize::from(size)).map_err(errno_err)
    }

    pub fn set_nonblocking(&self) -> Result<()> {
        let mut flags = rustix::fs::fcntl_getfl(self.as_fd()).map_err(errno_err)?;
        flags |= rustix::fs::OFlags::NONBLOCK;
        rustix::fs::fcntl_setfl(self.as_fd(), flags).map_err(errno_err)
    }

    pub fn into_owned_fd(self) -> OwnedFd {
        let PtyMaster(fd) = self;
        fd
    }
}

impl AsFd for PtyMaster {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl AsRawFd for PtyMaster {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

/// Slave end of a pseudo-terminal pair.
pub struct Pts(OwnedFd);

impl Pts {
    pub fn stdio_triple(&self) -> Result<(Stdio, Stdio, Stdio)> {
        let stdin = self.0.try_clone().map_err(std_io_err)?;
        let stdout = self.0.try_clone().map_err(std_io_err)?;
        let stderr = self.0.try_clone().map_err(std_io_err)?;
        Ok((stdin.into(), stdout.into(), stderr.into()))
    }

    pub fn session_leader_pre_exec(&self) -> impl FnMut() -> std::io::Result<()> + use<> {
        let pts_fd = self.0.as_raw_fd();
        move || {
            rustix::process::setsid().map_err(std::io::Error::from)?;
            // SAFETY: pts_fd is a valid fd borrowed from self (PtyMaster).
            // The BorrowedFd is used only within this closure, which runs
            // in a forked child before exec — self (and its fd) outlives
            // this call because the closure is consumed synchronously.
            rustix::process::ioctl_tiocsctty(unsafe { std::os::fd::BorrowedFd::borrow_raw(pts_fd) })
                .map_err(std::io::Error::from)?;
            Ok(())
        }
    }
}

/// Allocate a new PTY pair and apply the initial window size.
pub fn open_pty(size: PtySize) -> Result<(PtyMaster, Pts)> {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).map_err(errno_err)?;
    grantpt(&master).map_err(errno_err)?;
    unlockpt(&master).map_err(errno_err)?;

    let mut flags = rustix::io::fcntl_getfd(&master).map_err(errno_err)?;
    flags |= rustix::io::FdFlags::CLOEXEC;
    rustix::io::fcntl_setfd(&master, flags).map_err(errno_err)?;

    let master = PtyMaster(master);
    master.set_term_size(size)?;

    let pts_path = ptsname(master.as_fd(), vec![]).map_err(errno_err)?;
    let pts = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NOCTTY.bits() as i32)
        .open(OsStr::from_bytes(pts_path.as_bytes()))
        .map_err(std_io_err)?;

    Ok((master, Pts(pts.into())))
}

fn errno_err(errno: rustix::io::Errno) -> ExecError {
    ExecError::new(ExecErrorCode::SpawnError, errno.to_string())
}

fn std_io_err(error: std::io::Error) -> ExecError {
    ExecError::new(ExecErrorCode::SpawnError, error.to_string())
}
