use elph_ai::resilience::ResilienceConfig;
use elph_ai::{ClientIdentity, CreateModelsOptions, client_identity, create_models, set_client_identity};

#[test]
fn create_models_installs_process_identity() {
    let _ = create_models(Some(CreateModelsOptions {
        identity: Some(ClientIdentity::new("acme", "ACME")),
        ..Default::default()
    }));
    let id = client_identity();
    assert_eq!(id.product, "acme");
    assert_eq!(id.env_prefix, "ACME");
    assert_eq!(id.env_key("CACHE_RETENTION"), "ACME_CACHE_RETENTION");
    assert_eq!(id.env_key("GITHUB_HOST"), "ACME_GITHUB_HOST");
    set_client_identity(ClientIdentity::default());
}

#[test]
fn set_client_identity_changes_resilience_env_prefix() {
    set_client_identity(ClientIdentity::new("other", "OTHER"));
    unsafe {
        std::env::set_var("OTHER_RATE_LIMIT_DEMO_RPS", "42");
    }
    let config = ResilienceConfig::from_env("demo");
    unsafe {
        std::env::remove_var("OTHER_RATE_LIMIT_DEMO_RPS");
    }
    assert_eq!(config.requests_per_second, 42);
    set_client_identity(ClientIdentity::default());
}
