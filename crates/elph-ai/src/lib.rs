//! Unified LLM API with provider collections, automatic auth resolution, token and
//! cost tracking, and session context hand-off between models.
//!
//! Ported from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).
pub use anyhow::Result;

pub mod api;
pub mod auth;
pub mod images;
pub mod logger;
pub mod models;
pub mod providers;
pub mod resilience;
pub mod session_resources;
pub mod trace;
pub mod types;
pub mod utils;

pub use api::codex_transport::CodexWebSocketDebugStats;
pub use api::codex_transport::{close_codex_websocket_sessions, get_codex_websocket_debug_stats};
pub use api::codex_transport::{get_codex_websocket_input_delta, reset_codex_websocket_debug_stats};
pub use auth::oauth::OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD;
pub use auth::oauth::unregister_oauth_provider;
pub use auth::oauth::{OAuthApiKeyResult, OAuthProviderInterface, OPENAI_CODEX_BROWSER_LOGIN_METHOD};
pub use auth::oauth::{anthropic_oauth, builtin_oauth_provider_ids, get_oauth_api_key, get_oauth_provider};
pub use auth::oauth::{get_oauth_providers, github_copilot_oauth, oauth_provider_modify_models, openai_codex_oauth};
pub use auth::oauth::{refresh_oauth_token, register_oauth_provider, reset_oauth_providers};
pub use auth::{ApiKeyAuth, ApiKeyCredential, AuthContext, AuthResolveInput, AuthResult, BoxFuture, Credential};
pub use auth::{CredentialStore, DefaultAuthContext, InMemoryCredentialStore, ModelAuth, ModelsError};
pub use auth::{ModelsErrorCode, OAuthCredential, ProviderAuth};
pub use auth::{default_auth_context, env_api_key_auth, resolve_provider_auth};
pub use images::{CreateImagesModelsOptions, ImagesModels};
pub use images::{builtin_images_models, generate_images};
pub use models::{CreateModelsOptions, CreateProviderOptions, Models, MutableModels, Provider, ProviderApi};
pub use models::{calculate_cost, clamp_thinking_level, create_models, create_provider};
pub use models::{get_supported_thinking_levels, has_api, models_are_equal};
pub use providers::faux::{FauxModelDefinition, FauxProviderHandle, FauxResponseStep, RegisterFauxProviderOptions};
pub use providers::faux::{faux_assistant_message, faux_provider, faux_text, faux_thinking, faux_tool_call};
pub use providers::{builtin_models, get_builtin_model, get_builtin_models, get_builtin_providers};
pub use session_resources::SessionResourceCleanupRegistration;
pub use session_resources::{cleanup_session_resources, register_session_resource_cleanup};
pub use types::*;
pub use utils::deferred_tools::split_deferred_tools;
pub use utils::diagnostics::{append_assistant_message_diagnostic, create_assistant_message_diagnostic};
pub use utils::event_stream::EventStreamIterator;
pub use utils::{overflow, retry, validation};
