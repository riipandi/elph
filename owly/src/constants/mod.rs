//! Constants for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/constants.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Extended to support all providers available in `elph-ai`.

mod providers;
mod resolve;

pub use providers::{ProviderConfig, all_providers, provider_config};
pub use resolve::{
    get_provider_api_key, is_valid_model_id, normalize_model_id, provider_needs_api_key, provider_requires_base_url,
    resolve_configured_provider, resolve_model_id, resolve_provider_base_url,
};

/// The directory where documentation is stored
pub const OWLY_DIR: &str = "openwiki";

/// Path to the last update metadata file
pub const UPDATE_METADATA_PATH: &str = "openwiki/.last-update.json";

/// Owly version
pub const OWLY_VERSION: &str = "0.0.1";

/// Environment variable keys
pub const OWLY_PROVIDER_ENV_KEY: &str = "OWLY_PROVIDER";
pub const OWLY_MODEL_ID_ENV_KEY: &str = "OWLY_MODEL_ID";

/// Default provider
pub const DEFAULT_PROVIDER: &str = "opencode";

/// Default model ID (OpenCode Zen big-pickle)
pub const DEFAULT_MODEL_ID: &str = "big-pickle";

/// Environment variable for optional provider base URL override.
pub const ANTHROPIC_BASE_URL_ENV_KEY: &str = "ANTHROPIC_BASE_URL";
pub const OPENAI_BASE_URL_ENV_KEY: &str = "OPENAI_BASE_URL";
pub const OPENROUTER_BASE_URL_ENV_KEY: &str = "OPENROUTER_BASE_URL";
pub const OPENAI_COMPATIBLE_API_KEY_ENV_KEY: &str = "OPENAI_COMPATIBLE_API_KEY";
pub const OPENAI_COMPATIBLE_BASE_URL_ENV_KEY: &str = "OPENAI_COMPATIBLE_BASE_URL";

/// Providers offered in the first-run onboarding wizard.
pub const ONBOARDING_PROVIDERS: &[&str] = &[
    "opencode",
    "openrouter",
    "anthropic",
    "openai",
    "google",
    "deepseek",
    "groq",
    "fireworks",
];
