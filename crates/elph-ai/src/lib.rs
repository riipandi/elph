//! Unified LLM API with provider collections, automatic auth resolution, token and
//! cost tracking, and session context hand-off between models.
//!
//! Ported from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).
pub use anyhow::Result;

pub mod api;
pub mod auth;
pub mod images;
pub mod models;
pub mod providers;
pub mod types;
pub mod utils;

pub use api::codex_transport::{
    CodexWebSocketDebugStats, close_codex_websocket_sessions, get_codex_websocket_debug_stats,
    get_codex_websocket_input_delta, reset_codex_websocket_debug_stats,
};
pub use api::faux::FauxResponseStep;
pub use auth::oauth::{
    OAuthApiKeyResult, OAuthProviderInterface, OPENAI_CODEX_BROWSER_LOGIN_METHOD,
    OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD, anthropic_oauth, builtin_oauth_provider_ids, get_oauth_api_key,
    get_oauth_provider, get_oauth_providers, github_copilot_oauth, oauth_provider_modify_models, openai_codex_oauth,
    refresh_oauth_token, register_oauth_provider, reset_oauth_providers, unregister_oauth_provider,
};
pub use auth::{
    DefaultAuthContext, InMemoryCredentialStore, ModelsError, ModelsErrorCode, OAuthCredential, default_auth_context,
    env_api_key_auth, resolve_provider_auth,
};
pub use images::{CreateImagesModelsOptions, ImagesModels, builtin_images_models, generate_images};
pub use models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, Provider, calculate_cost, clamp_thinking_level,
    create_models, create_provider, get_supported_thinking_levels, has_api, models_are_equal,
};
pub use providers::faux::{
    FauxProviderHandle, faux_assistant_message, faux_provider, faux_text, faux_thinking, faux_tool_call,
};
pub use providers::{builtin_models, get_builtin_model, get_builtin_models, get_builtin_providers};
pub use types::*;
pub use utils::event_stream::EventStreamIterator;
pub use utils::{overflow, retry, validation};
