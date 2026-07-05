use std::future::Future;

use anyhow::Result;

/// Runs an async future on a current-thread Tokio runtime.
pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
        .block_on(future)
}

/// Runs an async future on a current-thread Tokio runtime.
pub fn try_block_on<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future))
}
