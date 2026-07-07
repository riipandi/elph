use anyhow::{Result, anyhow};
use url::Url;

use crate::types::ProviderEnv;
use crate::utils::provider_env::get_provider_env_value;

const DEFAULT_PROXY_PORTS: &[(&str, u16)] = &[
    ("ftp", 21),
    ("gopher", 70),
    ("http", 80),
    ("https", 443),
    ("ws", 80),
    ("wss", 443),
];

pub const UNSUPPORTED_PROXY_PROTOCOL_MESSAGE: &str =
    "Unsupported proxy protocol. SOCKS and PAC proxy URLs are not supported; use an HTTP or HTTPS proxy URL.";

fn get_proxy_env(key: &str, env: Option<&ProviderEnv>) -> String {
    let lowercase = key.to_lowercase();
    let uppercase = key.to_uppercase();

    if let Some(env) = env {
        if let Some(value) = env.get(&lowercase) {
            return value.clone();
        }
        if let Some(value) = env.get(&uppercase) {
            return value.clone();
        }
    }

    get_provider_env_value(&lowercase, None)
        .or_else(|| get_provider_env_value(&uppercase, None))
        .unwrap_or_default()
}

fn parse_proxy_target_url(target_url: &str) -> Option<Url> {
    Url::parse(target_url).ok()
}

fn default_port_for_protocol(protocol: &str) -> u16 {
    DEFAULT_PROXY_PORTS
        .iter()
        .find_map(|(name, port)| (*name == protocol).then_some(*port))
        .unwrap_or(0)
}

fn parse_no_proxy_entry(entry: &str) -> (String, u16) {
    if let Some((host, port_str)) = entry.rsplit_once(':')
        && let Ok(port) = port_str.parse::<u16>()
    {
        return (host.to_string(), port);
    }
    (entry.to_string(), 0)
}

fn should_proxy_hostname(hostname: &str, port: u16, env: Option<&ProviderEnv>) -> bool {
    let no_proxy = get_proxy_env("no_proxy", env).to_lowercase();
    if no_proxy.is_empty() {
        return true;
    }
    if no_proxy == "*" {
        return false;
    }

    no_proxy.split(|c: char| c == ',' || c.is_whitespace()).all(|proxy| {
        if proxy.is_empty() {
            return true;
        }

        let (mut proxy_hostname, proxy_port) = parse_no_proxy_entry(proxy);
        if proxy_port != 0 && proxy_port != port {
            return true;
        }

        let starts_with_wildcard = proxy_hostname.starts_with('.') || proxy_hostname.starts_with('*');
        if !starts_with_wildcard {
            return hostname != proxy_hostname;
        }

        if proxy_hostname.starts_with('*') {
            proxy_hostname = proxy_hostname[1..].to_string();
        }
        !hostname.ends_with(&proxy_hostname)
    })
}

fn get_proxy_for_url(target_url: &str, env: Option<&ProviderEnv>) -> String {
    let Some(parsed_url) = parse_proxy_target_url(target_url) else {
        return String::new();
    };

    let Some(protocol) = parsed_url.scheme().split(':').next() else {
        return String::new();
    };
    let Some(hostname) = parsed_url.host_str() else {
        return String::new();
    };

    let port = parsed_url.port().unwrap_or_else(|| default_port_for_protocol(protocol));
    if !should_proxy_hostname(hostname, port, env) {
        return String::new();
    }

    let protocol_proxy_key = format!("{protocol}_proxy");
    let mut proxy = get_proxy_env(&protocol_proxy_key, env);
    if proxy.is_empty() {
        proxy = get_proxy_env("all_proxy", env);
    }
    if proxy.is_empty() {
        return String::new();
    }
    if !proxy.contains("://") {
        proxy = format!("{protocol}://{proxy}");
    }
    proxy
}

/// Map a WebSocket URL to an HTTP(S) URL for proxy rule lookup (mirroring pi-ai Codex).
pub fn websocket_proxy_lookup_url(ws_url: &str) -> String {
    if let Some(rest) = ws_url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = ws_url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        ws_url.to_string()
    }
}

/// Resolve an HTTP or HTTPS proxy URL for `target_url` from scoped env and process env.
pub fn resolve_http_proxy_url_for_target(target_url: &str, env: Option<&ProviderEnv>) -> Result<Option<Url>> {
    let proxy = get_proxy_for_url(target_url, env);
    if proxy.is_empty() {
        return Ok(None);
    }

    let proxy_url = Url::parse(&proxy).map_err(|error| anyhow!("Invalid proxy URL {:?}: {error}", proxy))?;

    if proxy_url.scheme() != "http" && proxy_url.scheme() != "https" {
        return Err(anyhow!(
            "{UNSUPPORTED_PROXY_PROTOCOL_MESSAGE} Got {}:",
            proxy_url.scheme()
        ));
    }

    Ok(Some(proxy_url))
}
