//! Environment variable handling for CLI arguments.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

/// Load environment variables based on CLI arguments.
///
/// - If `no_global_env` is true, ignore system/OS environment variables
/// - If `env_file` is provided, load from file and supersede system env vars
/// - Otherwise, use default system environment
pub fn load_environment(no_global_env: bool, env_file: Option<&String>) -> Result<HashMap<String, String>> {
    let mut env_vars = HashMap::new();

    // Load system environment variables unless --no-global-env is set
    if !no_global_env {
        env_vars.extend(std::env::vars());
    }

    // Load from env file if provided (supersedes system env)
    if let Some(file_path) = env_file {
        let file_env = load_env_file(file_path)?;
        // Env file variables supersede system/OS variables
        env_vars.extend(file_env);
    }

    Ok(env_vars)
}

/// Load environment variables from a dotenv file.
fn load_env_file(file_path: &str) -> Result<HashMap<String, String>> {
    let path = Path::new(file_path);

    if !path.exists() {
        return Err(anyhow::anyhow!("Environment file not found: {}", file_path));
    }

    // Read and parse the dotenv file manually to avoid polluting process env
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read environment file: {}", file_path))?;

    let mut env_vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE format
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Remove quotes if present
            let value = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };

            if !key.is_empty() {
                env_vars.insert(key.to_string(), value.to_string());
            }
        }
    }

    Ok(env_vars)
}

/// Apply environment variables to the current process.
///
/// This function modifies the process environment variables based on the provided map.
/// Variables that don't exist in the map but exist in the current environment will be
/// removed if `no_global_env` was true, otherwise they remain.
pub fn apply_environment(env_vars: &HashMap<String, String>, no_global_env: bool) -> Result<()> {
    if no_global_env {
        // Clear all existing environment variables first
        for (key, _) in std::env::vars() {
            unsafe { std::env::remove_var(&key) };
        }
    }

    // Set the environment variables from our map
    for (key, value) in env_vars {
        unsafe { std::env::set_var(key, value) };
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_environment_default() {
        let env = load_environment(false, None).unwrap();
        // Should contain system environment variables
        assert!(!env.is_empty());
    }

    #[test]
    fn test_load_environment_no_global() {
        let env = load_environment(true, None).unwrap();
        // Should be empty when no global env and no file
        assert!(env.is_empty());
    }

    #[test]
    fn test_load_env_file() {
        // Create a temporary env file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "TEST_VAR=hello").unwrap();
        writeln!(temp_file, "ANOTHER_VAR=world").unwrap();

        let env = load_env_file(temp_file.path().to_str().unwrap()).unwrap();
        assert!(env.contains_key("TEST_VAR"));
        assert_eq!(env.get("TEST_VAR"), Some(&"hello".to_string()));
        assert!(env.contains_key("ANOTHER_VAR"));
        assert_eq!(env.get("ANOTHER_VAR"), Some(&"world".to_string()));
    }

    #[test]
    fn test_load_env_file_with_quotes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "QUOTED_VAR=\"quoted value\"").unwrap();
        writeln!(temp_file, "SINGLE_QUOTED='single quoted'").unwrap();

        let env = load_env_file(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(env.get("QUOTED_VAR"), Some(&"quoted value".to_string()));
        assert_eq!(env.get("SINGLE_QUOTED"), Some(&"single quoted".to_string()));
    }

    #[test]
    fn test_load_env_file_with_comments() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# This is a comment").unwrap();
        writeln!(temp_file, "VALID_VAR=value").unwrap();
        writeln!(temp_file, "").unwrap(); // empty line
        writeln!(temp_file, "ANOTHER_VAR=another").unwrap();

        let env = load_env_file(temp_file.path().to_str().unwrap()).unwrap();
        assert!(env.contains_key("VALID_VAR"));
        assert!(env.contains_key("ANOTHER_VAR"));
        assert!(!env.contains_key("# This is a comment"));
    }

    #[test]
    fn test_load_env_file_not_found() {
        let result = load_env_file("/nonexistent/path/.env");
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_environment() {
        let mut env_vars = HashMap::new();
        env_vars.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

        apply_environment(&env_vars, false).unwrap();
        assert_eq!(env::var("CUSTOM_VAR"), Ok("custom_value".to_string()));
    }

    #[test]
    fn test_env_file_supersedes_system() {
        // Set a system environment variable
        unsafe { env::set_var("TEST_OVERRIDE", "system_value") };

        // Create env file with same variable
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "TEST_OVERRIDE=file_value").unwrap();

        let env = load_environment(false, Some(&temp_file.path().to_string_lossy().to_string())).unwrap();
        // File value should supersede system value
        assert_eq!(env.get("TEST_OVERRIDE"), Some(&"file_value".to_string()));

        // Cleanup
        unsafe { env::remove_var("TEST_OVERRIDE") };
    }
}
