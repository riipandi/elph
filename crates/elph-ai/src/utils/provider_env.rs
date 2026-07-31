use crate::types::ProviderEnv;

pub fn get_provider_env_value(key: &str, env: Option<&ProviderEnv>) -> Option<String> {
    if let Some(env) = env {
        // When an explicit provider env map is provided, only use it — do not
        // fall back to the OS environment. This ensures callers who supply an
        // env map have full control over the environment they see.
        return env.get(key).cloned();
    }
    std::env::var(key).ok()
}
