//! Constants for Owly.
//!
//! Ported from [OpenWiki](https://github.com/langchain-ai/openwiki)
//! `src/constants.ts`. Original MIT License, Copyright (c) 2026 LangChain.
//!
//! Extended to support all providers available in `elph-ai`.

mod providers;
mod resolve;

pub use providers::{
    ProviderAuthMethod, ProviderConfig, ProviderModelOption, all_providers, is_known_provider, provider_config,
    provider_models_for_wizard, provider_oauth_capable, provider_oauth_only, provider_uses_oauth,
};
pub use resolve::{
    get_provider_api_key, is_valid_model_id, normalize_model_id, provider_is_configured, provider_needs_api_key,
    provider_requires_base_url, resolve_configured_provider, resolve_model_id, resolve_provider_base_url,
    resolve_provider_retry_attempts,
};

/// The directory where documentation is stored
pub const OWLY_DIR: &str = "openwiki";

/// Path to the last update metadata file (code mode, relative to repository root)
pub const UPDATE_METADATA_PATH: &str = "openwiki/.last-update.json";

/// Last-update metadata filename at the personal wiki root (`~/.owly/wiki/.last-update.json`)
pub const PERSONAL_UPDATE_METADATA_FILE: &str = ".last-update.json";

/// Owly version (crate package version)
pub const OWLY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Environment variable keys
pub const OWLY_PROVIDER_ENV_KEY: &str = "OWLY_PROVIDER";
pub const OWLY_MODEL_ID_ENV_KEY: &str = "OWLY_MODEL_ID";
pub const OWLY_PROVIDER_RETRY_ATTEMPTS_ENV_KEY: &str = "OWLY_PROVIDER_RETRY_ATTEMPTS";
pub const DEFAULT_PROVIDER_RETRY_ATTEMPTS: u32 = 3;

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

/// Providers offered in the first-run onboarding wizard (elph-ai backed).
pub const ONBOARDING_PROVIDERS: &[&str] = &[
    "opencode",
    "openrouter",
    "anthropic",
    "openai",
    "openai-codex",
    "openai-compatible",
    "google",
    "deepseek",
    "groq",
    "fireworks",
    "nvidia",
    "together",
    "mistral",
];
