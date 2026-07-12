pub mod prompt_encoding;

pub use prompt_encoding::{
    PromptEncodingConfig, PromptEncodingDelimiter, PromptEncodingMode, PromptEncodingTargets, ToonDecodeError,
    apply_to_tool_result, decode_toon_fence, encode_value, extract_json_value, parse_toon_fence,
};

use std::future::Future;

use anyhow::Result;

fn run_future<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return Ok(tokio::task::block_in_place(|| handle.block_on(future)));
    }

    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future))
}

/// Runs an async future, panicking if the runtime cannot be created.
pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    run_future(future).expect("failed to run async task")
}

/// Runs an async future, returning errors from runtime construction.
pub fn try_block_on<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    run_future(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_block_on_works_outside_runtime() {
        let value = try_block_on(async { 42 }).expect("outside runtime");
        assert_eq!(value, 42);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn try_block_on_works_inside_runtime() {
        let value = try_block_on(async { 42 }).expect("inside runtime");
        assert_eq!(value, 42);
    }
}
