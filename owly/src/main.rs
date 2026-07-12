//! Owly - Agent docs for codebases
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki) (`openwiki`).
//! The original code is licensed under the [MIT License](https://opensource.org/licenses/MIT).
//! Copyright (c) 2026 LangChain.
//!
//! This Rust port preserves the core design (agent-driven documentation
//! generation with filesystem tools, git integration, and update metadata)
//! with platform-specific adaptations for the Elph agent runtime.
//!
//! Uses `elph-agent` and `elph-ai` for agent execution and LLM provider
//! integration instead of the original LangChain/LangGraph implementation.
//!
//! Configuration is explicit: use CLI flags or environment variables.
//! No hidden state is maintained outside the working directory.

use clap::FromArgMatches;
use tracing_subscriber::{EnvFilter, fmt};

use owly::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing - suppress noisy logs by default
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("owly=info,fff_search=error,elph_agent=warn")),
        )
        .init();
    let cli = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&cli)?;

    cli.execute().await
}
