//! Integration test for SIGINT delivery to the TUI channel.

#[cfg(unix)]
#[test]
fn sigint_channel_receives_signal() {
    use std::time::Duration;

    fn raise_sigint() {
        unsafe {
            libc::raise(libc::SIGINT);
        }
    }

    elph_agent::block_on(async {
        let mut sigint = elph_tui::sigint_channel();
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            raise_sigint();
        });
        let received = tokio::time::timeout(Duration::from_secs(2), sigint.recv())
            .await
            .expect("timed out waiting for SIGINT on tokio runtime");
        assert!(received);
    });
}
