//! Shared HTTP, HTML, and URL helpers for web tools.

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use url::Url;

pub const FETCH_MAX_BYTES: usize = 256 * 1024;
/// Product UA for generic fetches (web_fetch / web_extract).
pub const USER_AGENT: &str = "Elph/1.0 (+https://elph.space)";
/// Browser-like UA for HTML search endpoints that bot-challenge the product UA.
pub const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .unwrap_or_else(|_| Client::new())
    })
}

/// Detect CAPTCHA / bot-challenge pages so callers fail clearly instead of "no results".
///
/// Matches common DuckDuckGo anomaly pages, Cloudflare challenges, and generic captcha
/// interstitials. Returns a short human-readable reason when blocked.
pub fn detect_bot_challenge(html: &str) -> Option<&'static str> {
    if html.is_empty() {
        return None;
    }
    let lower = html.to_ascii_lowercase();
    // DuckDuckGo HTML endpoint often serves an anomaly / captcha wall to datacenter IPs.
    if lower.contains("anomaly-modal")
        || lower.contains("anomaly.js")
        || lower.contains("please complete the following challenge")
        || lower.contains("unfortunately, bots use duckduckgo too")
        || lower.contains("name=\"challenge\"")
        || lower.contains("id=\"challenge-form\"")
    {
        return Some("DuckDuckGo bot/CAPTCHA challenge");
    }
    if lower.contains("cf-browser-verification")
        || lower.contains("cf-challenge")
        || lower.contains("checking your browser before accessing")
        || lower.contains("just a moment...") && (lower.contains("cloudflare") || lower.contains("cf-ray"))
    {
        return Some("Cloudflare bot challenge");
    }
    if lower.contains("g-recaptcha")
        || lower.contains("hcaptcha")
        || lower.contains("recaptcha/api")
        || lower.contains("captcha-delivery")
        || (lower.contains("are you a robot") && lower.contains("captcha"))
    {
        return Some("CAPTCHA challenge page");
    }
    if lower.contains("access denied") && (lower.contains("bot") || lower.contains("automated")) {
        return Some("bot access denied");
    }
    None
}

/// Error message when an HTML search endpoint is challenge-walled.
pub fn bot_challenge_error(provider: &str, reason: &str) -> anyhow::Error {
    anyhow!(
        "{provider} blocked by {reason}. Prefer an API-backed engine (brave, tavily, exa, serpapi, jina) \
         with its API key set, or retry later. Do not treat this as an empty result set."
    )
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

/// Truncate at a Unicode scalar boundary (never mid-codepoint).
pub(crate) fn truncate_at_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

fn trim_error_body(body: &str) -> String {
    let s = body.trim();
    if s.chars().count() > 240 {
        format!("{}...", truncate_at_chars(s, 240))
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

    #[test]
    fn truncate_at_chars_respects_scalar_boundaries() {
        let bullet = "•";
        let input = format!("{}a", bullet.repeat(100));
        let truncated = truncate_at_chars(&input, 100);
        assert_eq!(truncated.chars().count(), 100);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn trim_error_body_does_not_panic_on_multibyte_chars() {
        let body = format!("{}end", "•".repeat(250));
        let trimmed = trim_error_body(&body);
        assert!(trimmed.ends_with("..."));
        assert!(trimmed.chars().count() <= 244);
    }
}
