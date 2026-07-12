//! Owly - Agent docs for codebases
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! (`openwiki`). The original code is licensed under the
//! [MIT License](https://opensource.org/licenses/MIT).
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

pub mod agent;
pub mod ask_user;
pub mod auth;
pub mod checkpoint;
pub mod cli;
pub mod cli_product;
pub mod code_mode;
pub mod commands;
pub mod config;
pub mod connectors;
pub mod constants;
pub mod credentials;
pub mod diagnostics;
pub mod docs;
pub mod ecosystem;
pub mod env;
pub mod frontmatter;
pub mod help_content;
pub mod ingestion;
pub mod instructions;
pub mod interactive;
pub mod metadata;
pub mod mode;
pub mod onboarding;
pub mod onboarding_config;
pub mod prompts;
pub mod schedules;
pub mod session;
pub mod startup;
pub mod utils;
