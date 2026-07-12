use owly::config::Config;
use owly::constants::ONBOARDING_PROVIDERS;
use owly::constants::provider_is_configured;
use owly::credentials;
use owly::onboarding::{
    SetupCredentials, apply_setup, default_model_for_provider, needs_setup, provider_select_items,
    setup_base_url_required, setup_collects_base_url, setup_uses_oauth,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn test_config(provider: &str) -> Config {
    Config {
        provider: provider.to_string(),
        model_id: "big-pickle".to_string(),
        cwd: PathBuf::from("/tmp"),
    }
}

#[test]
fn provider_select_items_match_onboarding_list() {
    let items = provider_select_items();
    assert_eq!(items.len(), ONBOARDING_PROVIDERS.len());
    for (idx, id) in ONBOARDING_PROVIDERS.iter().enumerate() {
        assert_eq!(items[idx].0, *id);
        assert!(items[idx].1.contains(id));
    }
}

#[test]
fn default_model_for_known_provider() {
    let model = default_model_for_provider("opencode").expect("model");
    assert_eq!(model, "big-pickle");
}

#[test]
fn apply_setup_persists_credentials() {
    let home = TempDir::new().expect("tempdir");
    let prev_home = std::env::var("HOME").ok();
    // SAFETY: test runs single-threaded with exclusive HOME override.
    unsafe {
        std::env::set_var("HOME", home.path());
    }

    for key in credentials::MANAGED_ENV_KEYS {
        // SAFETY: clearing managed keys for isolated test env.
        unsafe {
            std::env::remove_var(key);
        }
    }

    let base = test_config("opencode");
    let next = apply_setup(
        SetupCredentials {
            provider: "opencode".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            model_id: "big-pickle".to_string(),
            oauth: false,
        },
        &base,
    )
    .expect("apply setup");

    assert_eq!(next.provider, "opencode");
    assert_eq!(next.model_id, "big-pickle");
    assert_eq!(std::env::var("OPENCODE_API_KEY").expect("api key"), "test-key");
    assert!(credentials::env_path().exists());

    if let Some(prev) = prev_home {
        // SAFETY: restoring HOME after test.
        unsafe {
            std::env::set_var("HOME", prev);
        }
    }
}

#[test]
fn apply_setup_rejects_empty_api_key() {
    let err = apply_setup(
        SetupCredentials {
            provider: "opencode".to_string(),
            api_key: "   ".to_string(),
            base_url: None,
            model_id: "big-pickle".to_string(),
            oauth: false,
        },
        &test_config("opencode"),
    )
    .expect_err("expected error");
    assert!(err.to_string().contains("API key is required"));
}

#[test]
fn needs_setup_when_api_key_missing() {
    let prev: HashMap<String, Option<String>> = credentials::MANAGED_ENV_KEYS
        .iter()
        .filter(|k| k.ends_with("_API_KEY"))
        .map(|k| (k.to_string(), std::env::var(k).ok()))
        .collect();

    for key in credentials::MANAGED_ENV_KEYS {
        if key.ends_with("_API_KEY") {
            // SAFETY: test isolation.
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    assert!(needs_setup(&test_config("opencode")));

    for (key, value) in prev {
        if let Some(value) = value {
            // SAFETY: restoring prior env.
            unsafe {
                std::env::set_var(&key, value);
            }
        }
    }
}

#[test]
fn anthropic_setup_uses_api_key_path() {
    assert!(!setup_uses_oauth("anthropic"));
}

#[test]
fn anthropic_is_configured_with_api_key_only() {
    let prev: HashMap<String, Option<String>> = credentials::MANAGED_ENV_KEYS
        .iter()
        .filter(|k| k.ends_with("_API_KEY"))
        .map(|k| (k.to_string(), std::env::var(k).ok()))
        .collect();

    for key in credentials::MANAGED_ENV_KEYS {
        if key.ends_with("_API_KEY") {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    }
    assert!(provider_is_configured("anthropic"));

    for (key, value) in prev {
        if let Some(value) = value {
            unsafe {
                std::env::set_var(&key, value);
            }
        } else {
            unsafe {
                std::env::remove_var(&key);
            }
        }
    }
}

#[test]
fn openai_compatible_requires_base_url_in_setup_helpers() {
    assert!(setup_collects_base_url("openai-compatible"));
    assert!(setup_base_url_required("openai-compatible"));
}
