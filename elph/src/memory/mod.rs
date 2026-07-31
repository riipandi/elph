mod capture;
mod cmd;
mod format;
pub mod hooks;
pub mod ops;
mod pack;
mod present;
mod rank;
pub mod runtime;
pub(crate) mod store;
pub mod tools;

pub use cmd::run;
pub use runtime::{MemoryRuntime, MemoryRuntimeOptions};

/// Run a memory slash command and return formatted output.
///
/// Supported subcommands:
///   status · list · recent · tasks · log · search · purge · flush · consolidate · help
///
/// `flush` is destructive: the TUI/CLI must confirm before calling this (or
/// [`ops::execute`] with [`ops::MemoryOp::Flush`]).
pub async fn slash_run(paths: &crate::platform::Paths, args: &str) -> Result<String, String> {
    let op = ops::MemoryOp::parse_slash(args)?;
    if matches!(op, ops::MemoryOp::Flush) {
        return Err("flush requires confirmation — use the TUI dialog or: elph memory flush".into());
    }
    ops::execute(paths, op).await.map_err(|e| e.to_string())
}

/// Counts for the flush confirmation dialog (best-effort; 0/0 if store unreadable).
pub async fn flush_preview(paths: &crate::platform::Paths) -> (u32, u32) {
    match store::open_store(paths, false) {
        Ok(store) => {
            if store.init().await.is_err() {
                return (0, 0);
            }
            match store.get_status().await {
                Ok(s) => (s.total_memories, s.total_tasks),
                Err(_) => (0, 0),
            }
        }
        Err(_) => (0, 0),
    }
}

/// Run a confirmed flush and return formatted result text.
pub async fn execute_flush(paths: &crate::platform::Paths) -> Result<String, String> {
    ops::execute(paths, ops::MemoryOp::Flush)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::ops::MemoryOp;

    #[test]
    fn slash_defaults_to_status() {
        assert!(matches!(MemoryOp::parse_slash("").unwrap(), MemoryOp::Status));
        assert!(matches!(MemoryOp::parse_slash("help").unwrap(), MemoryOp::Help));
    }
}
