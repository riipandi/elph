//! OAuth flows for Anthropic, GitHub Copilot, OpenAI Codex, xAI, Kimi, OpenRouter, and Radius.

mod anthropic;
mod callback;
mod device_code;
mod github_copilot;
mod hyper;
mod kimi;
mod openai_codex;
mod openrouter;
mod pages;
mod pkce;
mod radius;
mod registry;
mod xai;

pub use anthropic::{anthropic_oauth, anthropic_oauth_loader, login_anthropic, refresh_anthropic_token};
pub use github_copilot::{get_github_copilot_base_url, github_copilot_oauth, github_copilot_oauth_loader};
pub use github_copilot::{login_github_copilot, normalize_domain, refresh_github_copilot_token};
pub use hyper::refresh_hyper_token;
pub use hyper::{hyper_api_base_url, hyper_base_url, hyper_oauth, hyper_oauth_loader, hyper_user_agent, login_hyper};
pub use kimi::{kimi_oauth, kimi_oauth_loader};
pub use openai_codex::{OPENAI_CODEX_BROWSER_LOGIN_METHOD, OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD};
pub use openai_codex::{login_openai_codex, login_openai_codex_device_code, openai_codex_oauth};
pub use openai_codex::{openai_codex_oauth_loader, refresh_openai_codex_token};
pub use openrouter::{openrouter_oauth, openrouter_oauth_loader};
pub use radius::{radius_oauth, radius_oauth_loader};
pub use registry::unregister_oauth_provider;
pub use registry::{OAuthApiKeyResult, OAuthModifyModelsFn, OAuthProviderId, OAuthProviderInterface};
pub use registry::{builtin_oauth_provider_ids, get_oauth_api_key, get_oauth_provider, get_oauth_providers};
pub use registry::{github_copilot_catalog_models, oauth_provider_login, oauth_provider_modify_models};
pub use registry::{oauth_provider_to_auth, refresh_oauth_token, register_oauth_provider, reset_oauth_providers};
pub use xai::{xai_oauth, xai_oauth_loader};
