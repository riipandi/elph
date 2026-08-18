//! Unified LLM API: provider collections, auth resolution, streaming, and catalogs.
//!
//! Public surface is the crate root plus the modules listed below. Adapter internals
//! (`api` submodules that are not re-exported, `utils`) are not part of the contract.
//!
//! Ported from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).

pub mod api;
pub mod auth;
pub mod images;
pub mod models;
pub mod providers;
pub mod resilience;
pub mod session_resources;
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
