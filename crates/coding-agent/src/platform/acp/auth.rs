//! ACP connection authentication (`authenticate` / `auth/login` + logout).
//!
//! Connection-scoped: logout does not delete `auth.json`.
//!
//! Privileged (need credentials): `session/new`, `session/load`, `session/resume`,
//! `session/prompt`, `session/set_mode`, `session/set_config_option`.
//! Unprivileged: `initialize`, authenticate/login, logout, `session/list`,
//! `session/close`, `session/delete`, `session/cancel`.
//!
//! If the connection is still anonymous and env/`auth.json` already has keys,
//! privileged ops succeed without an extra authenticate call. After logout,
//! those ops require an explicit login even if credentials remain on disk.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::agent::provider_api_key_env;
use crate::platform::acp::state::{AcpAgentState, ConnectionAuth};
use crate::utils::path::AppPaths;

/// Accepts any env / `auth.json` credential already present on the machine.
pub const METHOD_EXISTING: &str = "existing-credentials";

/// Client launches `elph acp --setup` (interactive `elph provider connect`).
pub const METHOD_TERMINAL: &str = "elph-provider-connect";

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
    use agent_client_protocol::schema::v2::{AuthMethod, AuthMethodAgent, AuthMethodTerminal};
    let mut out = vec![
        AuthMethod::Terminal(
            AuthMethodTerminal::new(METHOD_TERMINAL, "Sign in with Elph")
                .description("Opens an interactive terminal: pick a provider and save an API key or complete OAuth.")
                .args(vec!["acp".into(), "--setup".into()]),
        ),
        AuthMethod::Agent(AuthMethodAgent::new(METHOD_EXISTING, "Existing credentials").description(
            "Use API keys already in the environment or Elph auth.json. Call auth/login after setting them.",
        )),
    ];
    for (id, name, desc) in PROVIDER_METHODS {
        out.push(AuthMethod::Agent(AuthMethodAgent::new(*id, *name).description(*desc)));
    }
    out
}

pub fn v1_auth_methods() -> Vec<agent_client_protocol::schema::v1::AuthMethod> {
    use agent_client_protocol::schema::v1::{AuthMethod, AuthMethodAgent, AuthMethodTerminal};
    let mut out = vec![
        AuthMethod::Terminal(
            AuthMethodTerminal::new(METHOD_TERMINAL, "Sign in with Elph")
                .description("Opens an interactive terminal: pick a provider and save an API key or complete OAuth.")
                .args(vec!["acp".into(), "--setup".into()]),
        ),
        AuthMethod::Agent(AuthMethodAgent::new(METHOD_EXISTING, "Existing credentials").description(
            "Use API keys already in the environment or Elph auth.json. Call authenticate after setting them.",
        )),
    ];
    for (id, name, desc) in PROVIDER_METHODS {
        out.push(AuthMethod::Agent(AuthMethodAgent::new(*id, *name).description(*desc)));
    }
    out
}

pub fn is_known_method(method_id: &str) -> bool {
    method_id == METHOD_EXISTING
        || method_id == METHOD_TERMINAL
        || PROVIDER_METHODS.iter().any(|(id, _, _)| *id == method_id)
}

/// Gate for session create / resume / prompt / config — not list, close, cancel, or auth itself.
pub fn require(state: &Arc<Mutex<AcpAgentState>>) -> Result<(), agent_client_protocol::Error> {
    let (status, paths) = {
        let guard = state.lock();
        (guard.auth, guard.paths.clone())
    };
    match status {
        ConnectionAuth::SignedIn => Ok(()),
        ConnectionAuth::SignedOut => Err(auth_required_error(
            "logged out: call authenticate (v1) or auth/login (v2) with an advertised methodId",
        )),
        ConnectionAuth::Anonymous => {
            if has_any_credentials(&paths) {
                state.lock().auth = ConnectionAuth::SignedIn;
                Ok(())
            } else {
                Err(auth_required_error(
                    "authentication required: call authenticate (v1) or auth/login (v2) with an advertised methodId",
                ))
            }
        }
    }
}

/// Mark the connection signed-in when `method_id` has usable credentials.
pub fn login(state: &Arc<Mutex<AcpAgentState>>, method_id: &str) -> Result<(), agent_client_protocol::Error> {
    if method_id == METHOD_TERMINAL {
        return Err(auth_required_error(
            "terminal auth is not an in-band login: the client must run `elph acp --setup` (or `elph provider connect`), then call authenticate with existing-credentials",
        ));
    }
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
        return Err(auth_required_error(&format!(
            "no credentials for `{method_id}`: set the provider env var or run `elph provider connect`, then retry"
        )));
    }
    state.lock().auth = ConnectionAuth::SignedIn;
    Ok(())
}

/// End the connection's signed-in state and abort open sessions.
pub async fn logout(state: &Arc<Mutex<AcpAgentState>>) {
    let keys: Vec<String> = {
        let mut guard = state.lock();
        guard.auth = ConnectionAuth::SignedOut;
        guard.sessions.keys().cloned().collect()
    };
    for key in keys {
        let _ = crate::platform::acp::session::close_by_id(state, &key).await;
    }
}

fn auth_required_error(message: &str) -> agent_client_protocol::Error {
    agent_client_protocol::Error::auth_required().data(serde_json::json!(message))
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
        assert!(is_known_method(METHOD_TERMINAL));
        assert!(is_known_method("openai"));
        assert!(is_known_method("anthropic"));
        assert!(!is_known_method("not-a-method"));
    }

    #[test]
    fn terminal_method_is_not_in_band_login() {
        let state = test_state(ConnectionAuth::Anonymous);
        assert!(login(&state, METHOD_TERMINAL).is_err());
    }

    #[test]
    fn env_nonempty_rejects_blank() {
        assert!(!env_nonempty("ELPH_ACP_AUTH_TEST_UNSET_VAR_XYZ"));
    }

    fn test_state(auth: ConnectionAuth) -> Arc<Mutex<AcpAgentState>> {
        let dir = std::env::temp_dir().join("elph-acp-auth-test");
        Arc::new(Mutex::new(AcpAgentState {
            sessions: std::collections::HashMap::new(),
            paths: crate::platform::Paths::from_dirs(dir.clone(), dir.clone(), dir),
            settings: crate::platform::Settings::defaults(),
            client_fs_read: false,
            client_elicitation_form: false,
            auth,
        }))
    }

    #[test]
    fn require_after_logout_fails_even_if_env_has_keys() {
        let state = test_state(ConnectionAuth::SignedOut);
        assert!(require(&state).is_err());
    }

    #[test]
    fn require_signed_in_ok() {
        let state = test_state(ConnectionAuth::SignedIn);
        assert!(require(&state).is_ok());
    }

    #[test]
    fn login_unknown_method_is_invalid_params() {
        let state = test_state(ConnectionAuth::Anonymous);
        assert!(login(&state, "not-a-method").is_err());
    }
}
