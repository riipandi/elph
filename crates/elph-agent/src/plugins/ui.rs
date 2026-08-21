//! UI hooks the host injects into extension guests (`notify` / `confirm`).

/// Callbacks from Wasm host functions into the product UI.
///
/// Headless / print / JSON modes should deny `confirm` and treat `notify` as a log line.
pub trait ExtensionUi: Send + Sync {
    fn notify(&self, message: &str, level: &str);
    /// Return `true` if the user approved. Default implementations must deny.
    fn confirm(&self, title: &str, body: &str) -> bool;
}

/// Safe default: log notifications, deny confirmations.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyUi;

impl ExtensionUi for DenyUi {
    fn notify(&self, message: &str, level: &str) {
        log::info!("extension notify level={level} {message}");
    }

    fn confirm(&self, title: &str, body: &str) -> bool {
        log::info!("extension confirm denied (no UI) title={title} body={body}");
        false
    }
}
