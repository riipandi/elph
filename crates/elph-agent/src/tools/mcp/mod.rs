//! MCP client integration via [rmcp](https://crates.io/crates/rmcp).
//!
//! # Features
//!
//! - **stdio**, **streamable HTTP**, and legacy **SSE** transports
//! - **OAuth 2.1** (PKCE) for remote servers via `mcp auth`
//! - **AES-256-GCM** credential encryption (`enc:` prefix) in shared `auth.json`
//! - **JSON Schema + semantic** config validation
//! - **Resources** and **prompts** bridge tools
//! - **Tool policy** (allow / deny / requireApproval)
//! - **Session pool** — reuse connections across tool calls
//! - **Hot reload** on `tools/list_changed` (and resource/prompt variants)
//! - **Agent tools** — `mcp_{server}__{tool}` naming for the model tool surface
//!
//! Enable with Cargo feature `mcp` (on by default).

#[cfg(feature = "mcp")]
mod auth;
#[cfg(feature = "mcp")]
mod auth_resolve;
#[cfg(feature = "mcp")]
mod client;
#[cfg(feature = "mcp")]
mod compat;
#[cfg(feature = "mcp")]
mod config;
#[cfg(feature = "mcp")]
mod crypto;
#[cfg(feature = "mcp")]
mod events;
#[cfg(feature = "mcp")]
mod policy;
#[cfg(feature = "mcp")]
mod registry;
#[cfg(feature = "mcp")]
mod session;
#[cfg(feature = "mcp")]
mod sse;
#[cfg(feature = "mcp")]
mod store_lock;
#[cfg(feature = "mcp")]
mod truncate;
#[cfg(feature = "mcp")]
mod validate;

#[cfg(feature = "mcp")]
pub use auth::AuthStoreFile;
#[cfg(feature = "mcp")]
pub use auth::AuthStorePathBuilder;
#[cfg(feature = "mcp")]
pub use auth::FileCredentialStore;
#[cfg(feature = "mcp")]
pub use auth::FileCredentialStoreBuilder;
#[cfg(feature = "mcp")]
pub use auth::McpOAuthFlowOptions;
#[cfg(feature = "mcp")]
pub use auth::McpOAuthFlowResult;
#[cfg(feature = "mcp")]
pub use auth::auth_store_path;
#[cfg(feature = "mcp")]
pub use auth::clear_credentials;
#[cfg(feature = "mcp")]
pub use auth::has_stored_credentials;
#[cfg(feature = "mcp")]
pub use auth::resolve_oauth_access_token;
#[cfg(feature = "mcp")]
pub use auth::run_oauth_flow;
#[cfg(feature = "mcp")]
pub use auth::run_oauth_flow_with_scopes;
#[cfg(feature = "mcp")]
pub use auth::{DEFAULT_AUTH_FILE_NAME, DEFAULT_OAUTH_SCOPES};
#[cfg(feature = "mcp")]
pub use auth_resolve::resolve_remote_auth;
#[cfg(feature = "mcp")]
pub use auth_resolve::{McpAuthSource, McpAuthSourceReport, ResolvedMcpAuth};
#[cfg(feature = "mcp")]
pub use client::PROBE_TIMEOUT;
#[cfg(feature = "mcp")]
pub use client::call_stdio_tool;
#[cfg(feature = "mcp")]
pub use client::call_tool_for_server;
#[cfg(feature = "mcp")]
pub use client::connect;
#[cfg(feature = "mcp")]
pub use client::connect_http;
#[cfg(feature = "mcp")]
pub use client::connect_stdio;
#[cfg(feature = "mcp")]
pub use client::connect_with_context;
#[cfg(feature = "mcp")]
pub use client::list_tools;
#[cfg(feature = "mcp")]
pub use client::list_tools_for_server;
#[cfg(feature = "mcp")]
pub use client::parse_stdio_config;
#[cfg(feature = "mcp")]
pub use client::probe_server;
#[cfg(feature = "mcp")]
pub use client::probe_server_with_auth;
#[cfg(feature = "mcp")]
pub use client::probe_stdio_server;
#[cfg(feature = "mcp")]
pub use client::shutdown_client;
#[cfg(feature = "mcp")]
pub use client::validate_server_config;
#[cfg(feature = "mcp")]
pub use client::{McpClient, McpConnectContext, McpProbeResult};
#[cfg(feature = "mcp")]
pub use config::DEFAULT_OPERATION_TIMEOUT_SECS;
#[cfg(feature = "mcp")]
pub use config::McpAuthConflictPolicy;
#[cfg(feature = "mcp")]
pub use config::McpConfig;
#[cfg(feature = "mcp")]
pub use config::McpHttpConfig;
#[cfg(feature = "mcp")]
pub use config::McpLoadOptions;
#[cfg(feature = "mcp")]
pub use config::McpOAuthClientMeta;
#[cfg(feature = "mcp")]
pub use config::McpServerConfig;
#[cfg(feature = "mcp")]
pub use config::McpStdioConfig;
#[cfg(feature = "mcp")]
pub use crypto::Aes256Key;
#[cfg(feature = "mcp")]
pub use crypto::decrypt_async;
#[cfg(feature = "mcp")]
pub use crypto::decrypt_json_async;
#[cfg(feature = "mcp")]
pub use crypto::decrypt_string_async;
#[cfg(feature = "mcp")]
pub use crypto::decrypt_string_sync;
#[cfg(feature = "mcp")]
pub use crypto::default_auth_key_path;
#[cfg(feature = "mcp")]
pub use crypto::encrypt_async;
#[cfg(feature = "mcp")]
pub use crypto::encrypt_json_async;
#[cfg(feature = "mcp")]
pub use crypto::encrypt_string_async;
#[cfg(feature = "mcp")]
pub use crypto::encrypt_string_sync;
#[cfg(feature = "mcp")]
pub use crypto::is_encrypted_value;
#[cfg(feature = "mcp")]
pub use crypto::{DEFAULT_AUTH_KEY_FILE_NAME, ENC_PREFIX};
#[cfg(feature = "mcp")]
pub use events::{McpClientService, McpEventBus, McpServerEvent};
#[cfg(feature = "mcp")]
pub use policy::{McpPolicyAction, McpPolicyConfig};
#[cfg(feature = "mcp")]
pub use policy::{mcp_tool_requires_approval, pattern_matches};
#[cfg(feature = "mcp")]
pub use registry::McpLoadReport;
#[cfg(feature = "mcp")]
pub use registry::McpPromptDescriptor;
#[cfg(feature = "mcp")]
pub use registry::McpResourceDescriptor;
#[cfg(feature = "mcp")]
pub use registry::McpServerLoadReport;
#[cfg(feature = "mcp")]
pub use registry::McpToolDescriptor;
#[cfg(feature = "mcp")]
pub use registry::McpToolRegistry;
#[cfg(feature = "mcp")]
pub use registry::{expose_tool_name, mcp_result_to_agent, mcp_result_to_agent_with_limit, parse_exposed_tool_name};
#[cfg(feature = "mcp")]
pub use session::{McpServerSession, McpSessionPool};
#[cfg(feature = "mcp")]
pub use truncate::truncate_chars;
#[cfg(feature = "mcp")]
pub use truncate::{DEFAULT_MAX_STRUCTURED_DETAIL_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS};
#[cfg(feature = "mcp")]
pub use validate::McpConfigValidationError;
#[cfg(feature = "mcp")]
pub use validate::parse_and_validate_mcp_config;
#[cfg(feature = "mcp")]
pub use validate::parse_and_validate_mcp_config_async;
#[cfg(feature = "mcp")]
pub use validate::parse_and_validate_server_config_json;
#[cfg(feature = "mcp")]
pub use validate::validate_mcp_config;
#[cfg(feature = "mcp")]
pub use validate::validate_mcp_config_semantic;
#[cfg(feature = "mcp")]
pub use validate::validate_mcp_config_value;
