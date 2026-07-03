mod app;
mod components;

use nix::sys::signal::{Signal, kill};
use nix::unistd::getppid;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(unix)]
static SHOULD_KILL_PARENT: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
fn kill_parent() {
    let ppid = getppid();
    if ppid.as_raw() > 1 {
        let _ = kill(ppid, Signal::SIGTERM);
    }
}

fn main() {
    app::run();

    #[cfg(unix)]
    if SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
        kill_parent();
    }
}
