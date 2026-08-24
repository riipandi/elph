use parking_lot::Mutex;

use elph_ai::OAuthCredential;
use elph_ai::auth::anthropic_oauth;
use elph_ai::auth::oauth::OAuthProviderInterface;
use elph_ai::auth::oauth::unregister_oauth_provider;
use elph_ai::auth::oauth::{builtin_oauth_provider_ids, get_oauth_provider, get_oauth_providers};
use elph_ai::auth::oauth::{oauth_provider_to_auth, register_oauth_provider, reset_oauth_providers};

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
    // Session-shaped token without proxy-ep (no network exchange).
    let access = "tid=abc;exp=123;sku=free";
    let enterprise = oauth_provider_to_auth(
        "github-copilot",
        OAuthCredential {
            kind: "oauth".to_string(),
            access: access.to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: Some("https://company.ghe.com".to_string()),
            available_model_ids: None,
        },
    )
    .await
    .expect("auth");
    assert_eq!(enterprise.base_url.as_deref(), Some("https://copilot-api.company.ghe.com"));

    let individual = oauth_provider_to_auth(
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
    assert_eq!(individual.base_url.as_deref(), Some("https://api.individual.githubcopilot.com"));
}

#[test]
fn oauth_registry_lists_builtin_providers() {
    let _guard = OAUTH_REGISTRY_LOCK.lock();
    reset_oauth_providers();
    assert_eq!(get_oauth_providers().len(), 10);
    for id in builtin_oauth_provider_ids() {
        assert!(get_oauth_provider(id).is_some(), "missing provider {id}");
    }
}

#[test]
fn github_copilot_modify_models_filters_by_available_ids() {
    use elph_ai::auth::oauth_provider_modify_models;
    use elph_ai::{OAuthCredential, get_builtin_models};

    let catalog = get_builtin_models("github-copilot");
    assert!(
        catalog.iter().any(|m| m.id == "auto"),
        "catalog should include Auto for Free/Student plans"
    );
    assert!(catalog.len() > 2, "expected full catalog");

    let filtered = oauth_provider_modify_models(
        "github-copilot",
        catalog.clone(),
        &OAuthCredential {
            kind: "oauth".to_string(),
            access: "tid=x;exp=1;proxy-ep=proxy.individual.githubcopilot.com".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: Some(vec!["auto".to_string(), "gpt-5-mini".to_string()]),
        },
    );
    let ids: std::collections::HashSet<_> = filtered.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, std::collections::HashSet::from(["auto", "gpt-5-mini"]));
    assert!(
        filtered
            .iter()
            .all(|m| m.base_url.contains("individual.githubcopilot.com")),
        "base_url should come from token proxy-ep"
    );

    let unfiltered = oauth_provider_modify_models(
        "github-copilot",
        catalog,
        &OAuthCredential {
            kind: "oauth".to_string(),
            access: "tid=x;exp=1;proxy-ep=proxy.individual.githubcopilot.com".to_string(),
            refresh: "r".to_string(),
            expires: 0,
            account_id: None,
            enterprise_url: None,
            available_model_ids: None,
        },
    );
    assert!(unfiltered.len() > 2, "None available_model_ids keeps full catalog");
}

#[test]
fn oauth_registry_register_and_unregister_custom_provider() {
    let _guard = OAUTH_REGISTRY_LOCK.lock();
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
