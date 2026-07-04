/// Receives `SIGINT` (Ctrl+C) on the active Tokio runtime.
pub struct SigintReceiver;

impl SigintReceiver {
    /// Waits until the next Ctrl+C signal.
    pub async fn recv(&mut self) -> bool {
        tokio::signal::ctrl_c().await.is_ok()
    }
}

/// Listens for `SIGINT` (Ctrl+C) via the Tokio signal driver.
pub fn sigint_channel() -> SigintReceiver {
    SigintReceiver
}
