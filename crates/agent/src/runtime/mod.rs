use std::future::Future;
use std::io;

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

/// Runs an async future, returning runtime construction errors as [`io::Error`].
pub fn try_block_on<F, T>(future: F) -> io::Result<T>
where
    F: Future<Output = T>,
{
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future))
}
