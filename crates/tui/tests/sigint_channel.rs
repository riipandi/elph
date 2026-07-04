#![cfg(unix)]

use elph_tui::sigint_channel;
use std::time::Duration;

#[tokio::test]
async fn sigint_channel_delivers_sigint_to_receiver() {
    let mut rx = sigint_channel();

    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(100));
        unsafe {
            libc::raise(libc::SIGINT);
        }
    });

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for SIGINT");

    assert!(received);
}
