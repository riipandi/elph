//! Run async connector work from sync ingest entrypoints (Owly main is async).

pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        // INVARIANT: sync connector ingest only; not called from inside a Tokio runtime.
        tokio::runtime::Runtime::new().expect("tokio runtime").block_on(future)
    }
}
