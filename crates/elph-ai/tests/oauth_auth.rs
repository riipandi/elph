use std::sync::Mutex;

use elph_ai::auth::oauth::{
    OAuthProviderInterface, builtin_oauth_provider_ids, get_oauth_provider, get_oauth_providers,
    oauth_provider_to_auth, register_oauth_provider, reset_oauth_providers, unregister_oauth_provider,
};
use elph_ai::{OAuthCredential, anthropic_oauth};

static OAUTH_REGISTRY_LOCK: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn anthropic_to_auth_uses_access_token() {
    let auth = oauth_provider_to_auth(
        "anthropic",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: "token".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(auth.api_key.as_deref(), Some("token"));
}

#[tokio::test]
async fn openai_codex_to_auth_uses_access_token() {
    let auth = oauth_provider_to_auth(
        "openai-codex",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: "token".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(auth.api_key.as_deref(), Some("token"));
}

#[tokio::test]
async fn github_copilot_to_auth_derives_base_url_from_proxy_endpoint() {
    let access = "tid=abc;exp=123;proxy-ep=proxy.enterprise.example;rest";
    let auth = oauth_provider_to_auth(
        "github-copilot",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: access.to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(auth.api_key.as_deref(), Some(access));
    assert_eq!(auth.base_url.as_deref(), Some("https://api.enterprise.example"));
}

#[tokio::test]
async fn github_copilot_to_auth_falls_back_to_enterprise_then_individual() {
    let enterprise = oauth_provider_to_auth(
        "github-copilot",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: "no-proxy-ep".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: Some("https://company.ghe.com".to_string()),
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(
        enterprise.base_url.as_deref(),
        Some("https://copilot-api.company.ghe.com")
    );

    let individual = oauth_provider_to_auth(
        "github-copilot",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: "no-proxy-ep".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(
        individual.base_url.as_deref(),
        Some("https://api.individual.githubcopilot.com")
    );
}

#[test]
fn oauth_registry_lists_builtin_providers() {
    let _guard = OAUTH_REGISTRY_LOCK.lock().unwrap();
    reset_oauth_providers();
    assert_eq!(get_oauth_providers().len(), 3);
    for id in builtin_oauth_provider_ids() {
        assert!(get_oauth_provider(id).is_some(), "missing provider {id}");
    }
}

#[test]
fn oauth_registry_register_and_unregister_custom_provider() {
    let _guard = OAUTH_REGISTRY_LOCK.lock().unwrap();
    reset_oauth_providers();
    register_oauth_provider(OAuthProviderInterface {
        id: "custom-oauth".to_string(),
        name: "Custom".to_string(),
        auth: anthropic_oauth(),
        get_api_key: std::sync::Arc::new(|c| c.access.clone()),
        modify_models: None,
    });
    assert!(get_oauth_provider("custom-oauth").is_some());
    unregister_oauth_provider("custom-oauth");
    assert!(get_oauth_provider("custom-oauth").is_none());
    unregister_oauth_provider("anthropic");
    assert_eq!(get_oauth_provider("anthropic").unwrap().id, "anthropic");
}
