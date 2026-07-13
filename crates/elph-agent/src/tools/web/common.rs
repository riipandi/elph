//! Shared HTTP, HTML, and URL helpers for web tools.

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use reqwest::Client;
use url::Url;

pub const FETCH_MAX_BYTES: usize = 256 * 1024;
pub const USER_AGENT: &str = "Elph/1.0 (+https://elph.space)";

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

pub async fn do_get(client: &Client, url: &str, headers: &[(&str, &str)]) -> Result<String> {
    let mut req = client.get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = crate::trace::with_trace_headers(req).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("HTTP {}: {}", status, trim_error_body(&body)));
    }
    Ok(resp.text().await?)
}

pub async fn do_post_json(
    client: &Client,
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut req = client.post(url).json(body);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = crate::trace::with_trace_headers(req).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("HTTP {}: {}", status, trim_error_body(&text)));
    }
    Ok(resp.json().await?)
}

fn trim_error_body(body: &str) -> String {
    let s = body.trim();
    if s.len() > 240 {
        format!("{}...", &s[..240])
    } else {
        s.to_string()
    }
}

pub fn strip_html(s: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    static WS_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| Regex::new(r"<[^>]*>").expect("tag regex"));
    let ws_re = WS_RE.get_or_init(|| Regex::new(r"\s+").expect("ws regex"));

    let mut result = tag_re.replace_all(s, "").to_string();
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");
    ws_re.replace_all(result.trim(), " ").trim().to_string()
}

pub fn html_to_text(data: &str) -> String {
    let clean = strip_html(data);
    clean
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn is_html_content_type(content_type: &str) -> bool {
    let ct = content_type.to_lowercase();
    ct.contains("text/html") || ct.contains("application/xhtml")
}

/// Test-only: allow loopback/private hosts.
#[cfg(test)]
pub static ALLOW_PRIVATE_HOSTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(test))]
const ALLOW_PRIVATE_HOSTS: bool = false;

pub async fn parse_public_url(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(anyhow!("empty URL"));
    }
    let parsed = Url::parse(raw).with_context(|| format!("invalid URL: {raw}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(anyhow!("only http and https URLs are supported")),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL missing host"))?
        .to_ascii_lowercase();

    let allow_private = {
        #[cfg(test)]
        {
            ALLOW_PRIVATE_HOSTS.load(std::sync::atomic::Ordering::Relaxed)
        }
        #[cfg(not(test))]
        {
            ALLOW_PRIVATE_HOSTS
        }
    };

    if !allow_private && (host == "localhost" || host.ends_with(".localhost")) {
        return Err(anyhow!("localhost URLs are not allowed"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip, allow_private) {
            return Err(anyhow!("private or reserved IP addresses are not allowed"));
        }
        return Ok(parsed);
    }

    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = format!("{host}:{port}")
        .to_socket_addrs()
        .with_context(|| format!("resolve host: {host}"))?;
    let mut found = false;
    for addr in addrs {
        found = true;
        if is_blocked_ip(addr.ip(), allow_private) {
            return Err(anyhow!("host resolves to private or reserved IP"));
        }
    }
    if !found {
        return Err(anyhow!("host resolved to no addresses"));
    }
    Ok(parsed)
}

fn is_blocked_ip(ip: IpAddr, allow_private: bool) -> bool {
    if allow_private {
        return false;
    }
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_private() || v4.is_link_local() {
                return true;
            }
            let octets = v4.octets();
            octets[0] == 0 || octets[0] == 127 || (octets[0] == 169 && octets[1] == 254)
        }
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_decodes_entities() {
        assert_eq!(strip_html("<b>hello</b>"), "hello");
        assert_eq!(strip_html("a &amp; b"), "a & b");
    }
}
