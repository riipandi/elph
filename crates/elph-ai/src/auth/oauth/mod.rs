//! OAuth flows for Anthropic, GitHub Copilot, and OpenAI Codex.

mod anthropic;
mod callback;
mod device_code;
mod github_copilot;
mod hyper;
mod openai_codex;
mod pages;
mod pkce;
mod registry;

pub use anthropic::{anthropic_oauth, anthropic_oauth_loader, login_anthropic, refresh_anthropic_token};
pub use github_copilot::{
    get_github_copilot_base_url, github_copilot_oauth, github_copilot_oauth_loader, login_github_copilot,
    normalize_domain, refresh_github_copilot_token,
};
pub use hyper::{
    hyper_api_base_url, hyper_base_url, hyper_oauth, hyper_oauth_loader, hyper_user_agent, login_hyper,
    refresh_hyper_token,
};
pub use openai_codex::{
    OPENAI_CODEX_BROWSER_LOGIN_METHOD, OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD, login_openai_codex,
    login_openai_codex_device_code, openai_codex_oauth, openai_codex_oauth_loader, refresh_openai_codex_token,
};
pub use registry::{
    OAuthApiKeyResult, OAuthModifyModelsFn, OAuthProviderId, OAuthProviderInterface, builtin_oauth_provider_ids,
    get_oauth_api_key, get_oauth_provider, get_oauth_providers, github_copilot_catalog_models, oauth_provider_login,
    oauth_provider_modify_models, oauth_provider_to_auth, refresh_oauth_token, register_oauth_provider,
    reset_oauth_providers, unregister_oauth_provider,
};
