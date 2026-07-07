mod common;

use std::collections::HashMap;

use common::fake_auth_context;
use elph_ai::providers::{
    amazon_bedrock_provider, anthropic_provider, builtin_models, builtin_providers, cloudflare_ai_gateway_provider,
    cloudflare_workers_ai_provider, google_vertex_provider,
};
use elph_ai::{CreateModelsOptions, create_models, get_builtin_model};

#[test]
fn builtin_models_registers_every_builtin_provider() {
    let models = builtin_models(None);
    let providers = models.get_providers();
    assert_eq!(providers.len(), builtin_providers().len());
    assert!(providers.iter().any(|p| p.id == "anthropic"));

    let anthropic = get_builtin_model("anthropic", "claude-3-5-sonnet-20241022").expect("model exists");
    assert_eq!(anthropic.api, "anthropic-messages");

    let all = models.get_models(None);
    assert!(all.len() > 500);

    for provider in providers {
        let list = models.get_models(Some(&provider.id));
        assert!(!list.is_empty(), "provider {} has no models", provider.id);
        assert!(
            list.iter().all(|m| m.provider == provider.id),
            "provider {} owns its models",
            provider.id
        );
    }
}

#[tokio::test]
async fn anthropic_auth_prefers_oauth_token_env() {
    let mut models = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([
                ("ANTHROPIC_API_KEY".to_string(), "key".to_string()),
                ("ANTHROPIC_OAUTH_TOKEN".to_string(), "oauth-token".to_string()),
            ]),
            vec![],
        )),
    }));
    models.set_provider(anthropic_provider());
    let model = models
        .get_models(Some("anthropic"))
        .into_iter()
        .next()
        .expect("anthropic model");

    let result = models.get_auth(&model).await.expect("auth").expect("configured");
    assert_eq!(result.auth.api_key.as_deref(), Some("oauth-token"));
    assert_eq!(result.source.as_deref(), Some("ANTHROPIC_OAUTH_TOKEN"));
}

#[tokio::test]
async fn bedrock_configured_from_aws_profile_without_api_key() {
    let mut models = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([("AWS_PROFILE".to_string(), "dev".to_string())]),
            vec![],
        )),
    }));
    models.set_provider(amazon_bedrock_provider());
    let model = models.get_models(Some("amazon-bedrock"))[0].clone();

    let result = models.get_auth(&model).await.expect("auth").expect("configured");
    assert!(result.auth.api_key.is_none());
    assert_eq!(result.source.as_deref(), Some("AWS_PROFILE"));

    let mut unconfigured = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(HashMap::new(), vec![])),
    }));
    unconfigured.set_provider(amazon_bedrock_provider());
    assert!(unconfigured.get_auth(&model).await.expect("auth").is_none());
}

#[tokio::test]
async fn cloudflare_workers_ai_requires_account_id() {
    let mut missing = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([("CLOUDFLARE_API_KEY".to_string(), "cf-key".to_string())]),
            vec![],
        )),
    }));
    missing.set_provider(cloudflare_workers_ai_provider());
    let model = missing.get_models(Some("cloudflare-workers-ai"))[0].clone();
    assert!(missing.get_auth(&model).await.expect("auth").is_none());

    let mut configured = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([
                ("CLOUDFLARE_API_KEY".to_string(), "cf-key".to_string()),
                ("CLOUDFLARE_ACCOUNT_ID".to_string(), "account-id".to_string()),
            ]),
            vec![],
        )),
    }));
    configured.set_provider(cloudflare_workers_ai_provider());
    let result = configured.get_auth(&model).await.expect("auth").expect("configured");
    assert_eq!(result.auth.api_key.as_deref(), Some("cf-key"));
    assert_eq!(
        result.auth.base_url.as_deref(),
        Some("https://api.cloudflare.com/client/v4/accounts/account-id/ai/v1")
    );
}

#[tokio::test]
async fn cloudflare_ai_gateway_requires_gateway_id() {
    let mut missing = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([
                ("CLOUDFLARE_API_KEY".to_string(), "cf-key".to_string()),
                ("CLOUDFLARE_ACCOUNT_ID".to_string(), "account-id".to_string()),
            ]),
            vec![],
        )),
    }));
    missing.set_provider(cloudflare_ai_gateway_provider());
    let model = get_builtin_model("cloudflare-ai-gateway", "claude-3-5-haiku").expect("anthropic gateway model");
    assert!(missing.get_auth(&model).await.expect("auth").is_none());

    let mut configured = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([
                ("CLOUDFLARE_API_KEY".to_string(), "cf-key".to_string()),
                ("CLOUDFLARE_ACCOUNT_ID".to_string(), "account-id".to_string()),
                ("CLOUDFLARE_GATEWAY_ID".to_string(), "gateway-id".to_string()),
            ]),
            vec![],
        )),
    }));
    configured.set_provider(cloudflare_ai_gateway_provider());
    let result = configured.get_auth(&model).await.expect("auth").expect("configured");
    assert_eq!(
        result
            .auth
            .headers
            .as_ref()
            .and_then(|h| h.get("cf-aig-authorization"))
            .and_then(|v| v.as_deref()),
        Some("Bearer cf-key")
    );
    assert_eq!(
        result.auth.base_url.as_deref(),
        Some("https://gateway.ai.cloudflare.com/v1/account-id/gateway-id/anthropic")
    );
}

#[tokio::test]
async fn vertex_resolves_via_adc_file_project_and_location() {
    let mut models = create_models(Some(CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([
                ("GOOGLE_CLOUD_PROJECT".to_string(), "my-project".to_string()),
                ("GOOGLE_CLOUD_LOCATION".to_string(), "us-central1".to_string()),
            ]),
            vec!["~/.config/gcloud/application_default_credentials.json"],
        )),
    }));
    models.set_provider(google_vertex_provider());
    let model = models.get_models(Some("google-vertex"))[0].clone();
    let result = models.get_auth(&model).await.expect("auth").expect("configured");
    assert!(result.auth.api_key.is_none());
    assert_eq!(result.source.as_deref(), Some("gcloud application default credentials"));
}
