//! Unified LLM client: provider collections, auth, streaming, and embedded catalogs.
//!
//! # Public contract
//!
//! Stable names live at the crate root and in:
//! [`types`], [`models`], [`providers`], [`auth`], [`api`] (re-exported API types),
//! [`images`], [`resilience`], [`estimate`].
//!
//! [`utils`] and [`trace`] are `#[doc(hidden)]` — not part of the contract.
//!
//! Chat streams finish in-band: [`AssistantMessageEvent::Error`] and
//! [`StopReason::Error`] / [`StopReason::Aborted`]. They are **not** `Result`.
//! Catalog, auth, and OAuth login/refresh return [`Result`]`<T, `[`ModelsError`]`>`.
//!
//! # Host identity
//!
//! [`ClientIdentity`] sets the product tag (Codex `originator`, xAI `referrer`) and
//! the process env prefix (`MYAPP` → `MYAPP_CACHE_RETENTION`, `MYAPP_GITHUB_HOST`,
//! `MYAPP_RATE_LIMIT_*`).
//!
//! Set [`CreateModelsOptions::identity`] on [`create_models`] / [`builtin_models`].
//! Pass the same identity to [`auth::oauth_provider_login`] and
//! [`resilience::ResilienceManager::with_env_prefix`]. It is not process-global.
//!
//! # Features
//!
//! - *(none)* — HTTP chat APIs, catalogs, faux, image HTTP
//! - `bedrock` — Amazon Bedrock SDK
//! - `oauth-callback` — local browser OAuth server
//! - `generate-models` — catalog generator binary
//! - `tracing` — fastrace
//!
//! # Example
//!
//! ```no_run
//! use elph_ai::{ClientIdentity, CreateModelsOptions, builtin_models};
//!
//! let models = builtin_models(Some(CreateModelsOptions {
//!     identity: Some(ClientIdentity::new("myapp", "MYAPP")),
//!     ..Default::default()
//! }));
//! # let _ = models;
//! ```
//!
//! Consumer notes: <https://github.com/riipandi/elph/blob/main/docs/elph-ai.md>
//!
//! Ported from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).

#![cfg_attr(docsrs, feature(doc_cfg))]

/// Provider wire adapters. Stable names are re-exported from this module;
/// other submodule paths are for adapters and in-tree tests.
pub mod api;
/// Credential stores, auth resolution, and OAuth login/refresh.
pub mod auth;
/// Image generation collections (OpenRouter images API).
pub mod images;
/// Model collections, catalogs, and provider factories.
pub mod models;
/// Built-in provider constructors and [`providers::builtin_models`].
pub mod providers;
/// Rate limits, circuit breaker, and retry.
pub mod resilience;
/// Session-scoped resource cleanup hooks (host integration).
pub mod session_resources;
/// Messages, models, stream options, and [`ClientIdentity`].
pub mod types;

/// Tracing hooks used by Elph hosts. Not part of the library contract.
#[doc(hidden)]
pub mod trace;
/// Adapter helpers. Not part of the public contract — use crate-root re-exports.
#[doc(hidden)]
pub mod utils;

pub use api::{wrap_on_payload, wrap_on_response};
pub use auth::{
    ApiKeyAuth, ApiKeyCredential, AuthContext, AuthResolveInput, AuthResult, BoxFuture, Credential, CredentialInfo,
    CredentialStore, DefaultAuthContext, InMemoryCredentialStore, InMemoryModelsStore, ModelAuth, ModelsError,
    ModelsErrorCode, ModelsStore, ModelsStoreEntry, OAuthCredential, ProviderAuth, ProviderStore, default_auth_context,
    env_api_key_auth, flexible_api_key_auth, is_local_or_loopback_base_url, optional_env_api_key_auth,
    resolve_provider_auth,
};
pub use images::{CreateImagesModelsOptions, ImagesModels, builtin_images_models, generate_images};
pub use models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, OverlayApplyReport, Provider, ProviderApi,
    ProviderUpdatePlan, ProviderUpdatePlanEntry, ProviderUpdateReport, ProviderUpdateStatus, UpdatePolicy,
    apply_provider_update, builtin_catalog, builtin_provider_ids, calculate_cost, clamp_thinking_level,
    create_disk_provider, create_models, create_provider, custom_provider_catalogs, custom_provider_ids,
    embedded_provider_ids, embedded_provider_json, get_supported_thinking_levels, has_api,
    install_provider_catalog_dir, invalidate_catalog_cache, map_thinking_level_for_api, merge_model_lists,
    models_are_equal, parse_provider_catalog_json, plan_provider_update, provider_catalog_dir,
    set_provider_catalog_dir,
};
pub use providers::faux::{
    FauxModelDefinition, FauxProviderHandle, FauxResponseStep, RegisterFauxProviderOptions, faux_assistant_message,
    faux_provider, faux_text, faux_thinking, faux_tool_call,
};
pub use providers::{builtin_models, get_builtin_model, get_builtin_models, get_builtin_providers};
pub use types::{
    Api, AssistantContentBlock, AssistantImages, AssistantMessage, AssistantMessageDiagnostic, AssistantMessageEvent,
    CacheRetention, ClientIdentity, ConstrainedSamplingConfig, ContentBlock, Context, GrammarVariants, ImageContent,
    ImagesApi, ImagesContext, ImagesModel, ImagesOptions, ImagesProviderId, Message, Model, ModelCost, ModelCostRates,
    ModelCostTier, ModelThinkingLevel, OnPayloadCallback, OnResponseCallback, ProviderEnv, ProviderHeaders, ProviderId,
    ProviderResponse, SimpleStreamOptions, StopReason, StreamOptions, TextContent, ThinkingBudgets, ThinkingContent,
    ThinkingLevel, ThinkingLevelMap, Tool, ToolCall, Transport, Usage, UsageCost, UserContent,
};
pub use utils::deferred_tools::split_deferred_tools;
pub use utils::diagnostics::{append_assistant_message_diagnostic, create_assistant_message_diagnostic};
pub use utils::estimate;
pub use utils::event_stream::{AssistantMessageEventStream, EventStreamIterator};
pub use utils::json_parse::parse_streaming_json;
pub use utils::retry;
pub use utils::text::{assistant_content_text, content_text};
pub use utils::validation::validate_tool_call;
