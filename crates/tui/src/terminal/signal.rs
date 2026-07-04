use signal_hook::consts::SIGINT;
use signal_hook::iterator::Signals;
use smol::channel;

/// Delivers `SIGINT` (Ctrl+C) to the async runtime.
pub fn sigint_channel() -> channel::Receiver<i32> {
    let (tx, rx) = channel::unbounded();

    std::thread::spawn(move || {
        let mut signals = match Signals::new([SIGINT]) {
            Ok(signals) => signals,
            Err(_) => return,
        };

        for signal in signals.forever() {
            if tx.send_blocking(signal).is_err() {
                break;
            }
        }
    });

    rx
}
