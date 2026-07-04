use crossbeam::channel::{Receiver, TryRecvError, unbounded};
use signal_hook::consts::SIGINT;

/// Receives `SIGINT` (Ctrl+C) delivered from a background listener thread.
pub struct SigintReceiver {
    inner: Receiver<i32>,
}

impl SigintReceiver {
    /// Waits for the next signal on the async runtime.
    pub async fn recv(&mut self) -> Option<i32> {
        loop {
            match self.inner.try_recv() {
                Ok(signal) => return Some(signal),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => {
                    tokio::task::yield_now().await;
                }
            }
        }
    }
}

/// Delivers `SIGINT` (Ctrl+C) to the async runtime via a crossbeam channel.
pub fn sigint_channel() -> SigintReceiver {
    let (tx, rx) = unbounded();

    std::thread::spawn(move || {
        run_sigint_listener(tx);
    });

    SigintReceiver { inner: rx }
}

#[cfg(unix)]
fn run_sigint_listener(tx: crossbeam::channel::Sender<i32>) {
    use signal_hook::iterator::Signals;

    let mut signals = match Signals::new([SIGINT]) {
        Ok(signals) => signals,
        Err(_) => return,
    };

    for signal in signals.forever() {
        if tx.send(signal).is_err() {
            break;
        }
    }
}

#[cfg(windows)]
fn run_sigint_listener(tx: crossbeam::channel::Sender<i32>) {
    use signal_hook::flag;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    let flag = Arc::new(AtomicBool::new(false));
    if flag::register(SIGINT, Arc::clone(&flag)).is_err() {
        return;
    }

    loop {
        if flag.swap(false, Ordering::Relaxed) {
            if tx.send(SIGINT).is_err() {
                break;
            }
            continue;
        }
        thread::sleep(Duration::from_millis(25));
    }
}
