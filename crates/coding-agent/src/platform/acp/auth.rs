//! ACP connection authentication (`authenticate` / `auth/login` + logout).
//!
//! Connection-scoped: logout does not delete `auth.json`. After logout, session
//! operations fail with `auth_required` until the client logs in again.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::agent::provider_api_key_env;
use crate::platform::acp::state::AcpAgentState;
use crate::utils::path::AppPaths;

/// Accepts any env / `auth.json` credential already present on the machine.
pub const METHOD_EXISTING: &str = "existing-credentials";

/// Provider-specific `type: agent` methods advertised to clients.
const PROVIDER_METHODS: &[(&str, &str, &str)] = &[
    ("openai", "OpenAI API key", "Use OPENAI_API_KEY or a stored OpenAI credential."),
    (
        "anthropic",
        "Anthropic API key",
        "Use ANTHROPIC_API_KEY or a stored Anthropic credential.",
    ),
    ("xai", "xAI API key", "Use XAI_API_KEY or a stored xAI credential."),
    (
        "openrouter",
        "OpenRouter API key",
        "Use OPENROUTER_API_KEY or a stored OpenRouter credential.",
    ),
    (
        "github-copilot",
        "GitHub Copilot",
        "Use COPILOT_GITHUB_TOKEN / GITHUB_TOKEN or a stored Copilot credential.",
    ),
];

pub fn v2_auth_methods() -> Vec<agent_client_protocol::schema::v2::AuthMethod> {
    use agent_client_protocol::schema::v2::{AuthMethod, AuthMethodAgent};
    let mut out = vec![AuthMethod::Agent(
        AuthMethodAgent::new(METHOD_EXISTING, "Existing credentials").description(
            "Use API keys already in the environment or Elph auth.json. Call auth/login after setting them.",
        ),
    )];
    for (id, name, desc) in PROVIDER_METHODS {
        out.push(AuthMethod::Agent(AuthMethodAgent::new(*id, *name).description(*desc)));
    }
    out
}

pub fn v1_auth_methods() -> Vec<agent_client_protocol::schema::v1::AuthMethod> {
    use agent_client_protocol::schema::v1::{AuthMethod, AuthMethodAgent};
    let mut out = vec![AuthMethod::Agent(
        AuthMethodAgent::new(METHOD_EXISTING, "Existing credentials").description(
            "Use API keys already in the environment or Elph auth.json. Call authenticate after setting them.",
        ),
    )];
    for (id, name, desc) in PROVIDER_METHODS {
        out.push(AuthMethod::Agent(AuthMethodAgent::new(*id, *name).description(*desc)));
    }
    out
}

pub fn is_known_method(method_id: &str) -> bool {
    method_id == METHOD_EXISTING || PROVIDER_METHODS.iter().any(|(id, _, _)| *id == method_id)
}

pub fn require(state: &Arc<Mutex<AcpAgentState>>) -> Result<(), agent_client_protocol::Error> {
    if state.lock().authenticated {
        Ok(())
    } else {
        Err(agent_client_protocol::Error::auth_required().data(serde_json::json!(
            "authentication required: call authenticate (v1) or auth/login (v2) with an advertised methodId"
        )))
    }
}

/// Mark the connection authenticated when `method_id` has usable credentials.
pub fn login(state: &Arc<Mutex<AcpAgentState>>, method_id: &str) -> Result<(), agent_client_protocol::Error> {
    if !is_known_method(method_id) {
        return Err(agent_client_protocol::Error::invalid_params()
            .data(serde_json::json!(format!("unknown auth method `{method_id}`"))));
    }
    let paths = state.lock().paths.clone();
    let ok = if method_id == METHOD_EXISTING {
        has_any_credentials(&paths)
    } else {
        has_provider_credentials(&paths, method_id)
    };
    if !ok {
        return Err(agent_client_protocol::Error::auth_required().data(serde_json::json!(format!(
            "no credentials for `{method_id}`: set the provider env var or run `elph provider connect`, then retry"
        ))));
    }
    state.lock().authenticated = true;
    Ok(())
}

/// End the connection's authenticated state and abort open sessions.
pub async fn logout(state: &Arc<Mutex<AcpAgentState>>) {
    let keys: Vec<String> = {
        let mut guard = state.lock();
        guard.authenticated = false;
        guard.sessions.keys().cloned().collect()
    };
    for key in keys {
        let _ = crate::platform::acp::session::close_by_id(state, &key).await;
    }
}

pub fn has_any_credentials(paths: &crate::platform::Paths) -> bool {
    if PROVIDER_METHODS.iter().any(|(id, _, _)| env_set_for_provider(id)) {
        return true;
    }
    auth_store_has_any(&paths.auth_store_path())
}

fn has_provider_credentials(paths: &crate::platform::Paths, provider_id: &str) -> bool {
    env_set_for_provider(provider_id) || auth_store_has_provider(&paths.auth_store_path(), provider_id)
}

fn env_set_for_provider(provider_id: &str) -> bool {
    let Some(var) = provider_api_key_env(provider_id) else {
        return false;
    };
    if env_nonempty(var) {
        return true;
    }
    if provider_id == "github-copilot" {
        return env_nonempty("GITHUB_TOKEN");
    }
    false
}

fn env_nonempty(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.trim().is_empty())
}

fn auth_store_has_any(path: &Path) -> bool {
    load_auth_providers(path).is_some_and(|map| !map.is_empty())
}

fn auth_store_has_provider(path: &Path, provider_id: &str) -> bool {
    load_auth_providers(path).is_some_and(|map| map.contains_key(provider_id))
}

fn load_auth_providers(path: &Path) -> Option<std::collections::BTreeMap<String, serde_json::Value>> {
    if let Ok(file) = elph_agent::AuthStoreFile::load_from_path_sync(path) {
        return Some(file.provider);
    }
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("provider")
        .or_else(|| json.get("providers"))
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_methods_include_existing_and_providers() {
        assert!(is_known_method(METHOD_EXISTING));
        assert!(is_known_method("openai"));
        assert!(is_known_method("anthropic"));
        assert!(!is_known_method("not-a-method"));
    }

    #[test]
    fn env_nonempty_rejects_blank() {
        assert!(!env_nonempty("ELPH_ACP_AUTH_TEST_UNSET_VAR_XYZ"));
    }
}
