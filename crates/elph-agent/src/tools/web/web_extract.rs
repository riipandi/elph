//! `web_extract` agent tool.
//!
//! Fetches a public URL and extracts *structured* data from the DOM using the
//! `astral-tl` CSS-selector engine: links (with absolute URLs), images,
//! cleaned text, and the matched elements (tag + attributes + text + HTML).
//! Unlike `web_fetch` (Markdown conversion), this tool returns machine-readable
//! JSON for downstream tooling/agents.

use serde_json::Value;
use serde_json::json;
use tl::ParserOptions;

use elph_ai::Tool;

use crate::tools::common::check_aborted;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::common::{FETCH_MAX_BYTES, parse_public_url};
use super::html::fetch_raw;

/// Default and hard cap on how many links/elements/images to return.
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
/// Cap on the returned cleaned-text length (chars), to bound very large pages.
const MAX_TEXT_CHARS: usize = 32_000;

/// What the caller may ask to extract. Defaults to links, text, and elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractKind {
    Links,
    Images,
    Text,
    Elements,
}

impl ExtractKind {
    fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "links" | "link" | "anchors" | "anchor" => Some(Self::Links),
            "images" | "image" | "imgs" | "img" => Some(Self::Images),
            "text" | "content" => Some(Self::Text),
            "elements" | "element" | "nodes" | "node" => Some(Self::Elements),
            _ => None,
        }
    }
}

pub fn create_web_extract_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "web_extract".into(),
            constrained_sampling: None,
            description: "Extracts structured data from a web page: links (with absolute URLs), images, cleaned text, and matched elements (tag, attributes, text, HTML). Use a CSS `selector` to scope extraction to a subtree, and `extract` to pick which data to return (links, images, text, elements). Useful for scraping/mining specific page structure rather than reading prose.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTP or HTTPS URL to extract from"
                    },
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector to scope extraction (e.g. \"article\", \".product\", \"#main\"). When set, the `elements` field returns matches and links/images/text are read from within that subtree."
                    },
                    "extract": {
                        "type": "array",
                        "description": "Which data to extract. Defaults to [\"links\", \"text\", \"elements\"]. Allowed: links, images, text, elements.",
                        "items": { "type": "string" }
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of links/elements/images to return (default: 100, max: 1000)"
                    }
                },
                "required": ["url"]
            }),
        },
        "web_extract",
        |_, args| Box::pin(async move { execute_web_extract(args, None).await }),
    )
}

async fn execute_web_extract(
    args: Value,
    signal: Option<tokio_util::sync::CancellationToken>,
) -> anyhow::Result<AgentToolResult> {
    check_aborted(signal.as_ref())?;

    let raw_url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: url"))?;
    if raw_url.trim().is_empty() {
        return Err(anyhow::anyhow!("Empty url"));
    }

    // Resolve + SSRF-check the URL before any network call.
    let parsed = parse_public_url(raw_url).await?;
    let base_url = parsed.as_str().to_string();

    let selector = args
        .get("selector")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let kinds = parse_extract(&args);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64)
        .clamp(1, MAX_LIMIT as u64) as usize;

    let page = fetch_raw(parsed.as_str()).await?;
    let dom =
        tl::parse(&page.html, ParserOptions::default()).map_err(|e| anyhow::anyhow!("failed to parse HTML: {e}"))?;
    let parser = dom.parser();

    let mut output = serde_json::Map::new();
    output.insert("url".into(), json!(&page.url));
    output.insert("content_type".into(), json!(&page.content_type));
    if let Some(title) = extract_title(&dom, parser) {
        output.insert("title".into(), json!(title));
    }
    if let Some(selector) = &selector {
        output.insert("selector".into(), json!(selector));
    }

    // Roots drive text extraction; `NodeHandle`s drive structured extraction
    // (correctly scoped to the subtree via `collect_subtree_handles`).
    let roots: Vec<&tl::Node> = if let Some(selector) = &selector {
        match dom.query_selector(selector) {
            Some(matches) => matches.filter_map(|h| h.get(parser)).collect(),
            None => {
                return Err(anyhow::anyhow!("invalid or unsupported CSS selector: {selector}"));
            }
        }
    } else {
        dom.nodes().iter().collect()
    };

    // Collect every tag handle within the scoped subtree(s).
    // Collect every tag handle within the scoped subtree(s).
    let mut element_handles: Vec<tl::NodeHandle> = Vec::new();
    match &selector {
        Some(sel) => {
            if let Some(matches) = dom.query_selector(sel) {
                for handle in matches {
                    collect_subtree_handles(handle, parser, &mut element_handles);
                }
            }
        }
        None => {
            if let Some(matches) = dom.query_selector("*") {
                element_handles = matches.collect();
            }
        }
    }
    let all_tags: Vec<&tl::Node> = element_handles.iter().filter_map(|h| h.get(parser)).collect();

    if kinds.contains(&ExtractKind::Links) {
        let links = extract_links(&all_tags, parser, &base_url, limit);
        output.insert("links".into(), json!(links));
    }
    if kinds.contains(&ExtractKind::Images) {
        let images = extract_images(&all_tags, &base_url, limit);
        output.insert("images".into(), json!(images));
    }
    if kinds.contains(&ExtractKind::Text) {
        let text = extract_text(&roots, parser);
        output.insert("text".into(), json!(text));
    }
    if kinds.contains(&ExtractKind::Elements) {
        let elements = extract_elements(&all_tags, parser, limit);
        output.insert("elements".into(), json!(elements));
    }

    // Guard against huge payloads; truncate text defensively if present.
    let mut rendered = serde_json::to_string_pretty(&Value::Object(output)).unwrap_or_else(|_| "{}".to_string());
    if rendered.len() > FETCH_MAX_BYTES {
        rendered.truncate(FETCH_MAX_BYTES);
        rendered.push_str("\n\n(output truncated to fit size limit)");
    }

    Ok(AgentToolResult::text(rendered))
}

/// Parse the `extract` argument (array or single string) into a set of kinds.
/// Defaults to links + text + elements when absent or invalid.
fn parse_extract(args: &Value) -> Vec<ExtractKind> {
    let mut kinds = Vec::new();
    match args.get("extract") {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(k) = item.as_str().and_then(ExtractKind::from_str) {
                    kinds.push(k);
                }
            }
        }
        Some(Value::String(s)) => {
            for part in s.split(',') {
                if let Some(k) = ExtractKind::from_str(part) {
                    kinds.push(k);
                }
            }
        }
        _ => {}
    }
    if kinds.is_empty() {
        kinds = vec![ExtractKind::Links, ExtractKind::Text, ExtractKind::Elements];
    }
    kinds.sort_by_key(|k| *k as u8);
    kinds.dedup();
    kinds
}

/// Best-effort `<title>` extraction.
fn extract_title<'a>(dom: &'a tl::VDom, parser: &'a tl::Parser) -> Option<String> {
    let handle = dom.query_selector("title")?.next()?;
    let node = handle.get(parser)?;
    let text = node.inner_text(parser).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Recursively collect every tag `NodeHandle` within the subtree rooted at
/// `handle` (including `handle` itself). Walking `HTMLTag::children().top()`
/// (direct children) and descending keeps extraction correctly scoped to the
/// subtree, unlike `HTMLTag::query_selector` which matches document-wide.
fn collect_subtree_handles(handle: tl::NodeHandle, parser: &tl::Parser, out: &mut Vec<tl::NodeHandle>) {
    out.push(handle);
    if let Some(node) = handle.get(parser)
        && let Some(tag) = node.as_tag()
    {
        for child in tag.children().top().iter() {
            collect_subtree_handles(*child, parser, out);
        }
    }
}

fn extract_links(tags: &[&tl::Node], parser: &tl::Parser, base_url: &str, limit: usize) -> Vec<Value> {
    let mut out = Vec::with_capacity(limit.min(64));
    for node in tags {
        if out.len() >= limit {
            break;
        }
        let Some(tag) = node.as_tag() else { continue };
        if tag.name().as_utf8_str() != "a" {
            continue;
        }
        let Some(href) = attr_str(tag, "href") else { continue };
        if href.is_empty() {
            continue;
        }
        let absolute = resolve_url(base_url, &href);
        let text = node.inner_text(parser).trim().to_string();
        out.push(json!({
            "href": absolute,
            "text": text,
        }));
    }
    out
}

fn extract_images(tags: &[&tl::Node], base_url: &str, limit: usize) -> Vec<Value> {
    let mut out = Vec::with_capacity(limit.min(64));
    for node in tags {
        if out.len() >= limit {
            break;
        }
        let Some(tag) = node.as_tag() else { continue };
        if tag.name().as_utf8_str() != "img" {
            continue;
        }
        let Some(src) = attr_str(tag, "src") else { continue };
        if src.is_empty() {
            continue;
        }
        let absolute = resolve_url(base_url, &src);
        let alt = attr_str(tag, "alt").unwrap_or_default();
        out.push(json!({
            "src": absolute,
            "alt": alt,
        }));
    }
    out
}

/// Concatenate the cleaned text of every scoped subtree, collapsing whitespace.
fn extract_text(scope: &[&tl::Node], parser: &tl::Parser) -> String {
    let mut buf = String::new();
    for node in scope {
        let text = node.inner_text(parser);
        if !text.trim().is_empty() {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(text.trim());
        }
    }
    let collapsed = collapse_ws(&buf);
    if collapsed.chars().count() > MAX_TEXT_CHARS {
        let truncated: String = collapsed.chars().take(MAX_TEXT_CHARS).collect();
        format!("{truncated}\n\n(text truncated)")
    } else {
        collapsed
    }
}

/// Serialize the given nodes as elements (tags only; raw text/comment nodes
/// are skipped). Used for both matched-selector nodes and whole-document
/// element dumps.
fn extract_elements(nodes: &[&tl::Node], parser: &tl::Parser, limit: usize) -> Vec<Value> {
    let mut out = Vec::with_capacity(limit.min(64));
    for node in nodes {
        if out.len() >= limit {
            return out;
        }
        if let Some(tag) = node.as_tag() {
            out.push(element_to_json(tag, parser));
        }
    }
    out
}

/// Serialize a single matched element: tag, attributes, text, and inner HTML.
fn element_to_json(tag: &tl::HTMLTag, parser: &tl::Parser) -> Value {
    let name = tag.name().as_utf8_str().to_string();

    let mut attributes = serde_json::Map::new();
    for (key, value) in tag.attributes().iter() {
        attributes.insert(key.to_string(), json!(value.map(|v| v.to_string()).unwrap_or_default()));
    }

    let text = tag.inner_text(parser).trim().to_string();
    let html = tag.inner_html(parser).to_string();

    json!({
        "tag": name,
        "attributes": Value::Object(attributes),
        "text": text,
        "html": html,
    })
}

/// Read a single attribute value as an owned String, if present and valued.
fn attr_str<'a>(tag: &'a tl::HTMLTag<'a>, key: &str) -> Option<String> {
    tag.attributes().get(key).flatten().map(|b| b.as_utf8_str().to_string())
}

/// Resolve a possibly-relative href/src against the page base URL.
fn resolve_url(base: &str, target: &str) -> String {
    match url::Url::parse(base).ok().and_then(|b| b.join(target).ok()) {
        Some(resolved) => resolved.to_string(),
        None => target.to_string(),
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<!doctype html>
<html>
  <head><title>Example Page</title></head>
  <body>
    <nav><a href="/skip">Skip</a></nav>
    <main id="main">
      <h1>Hello World</h1>
      <p class="lead">Some <b>bold</b> text here.</p>
      <a href="https://example.com/about">About Us</a>
      <a href="/contact">Contact</a>
      <img src="/logo.png" alt="Logo" />
    </main>
  </body>
</html>"#;

    fn parse() -> tl::VDom<'static> {
        tl::parse(SAMPLE, ParserOptions::default()).unwrap()
    }

    #[test]
    fn extracts_title_and_links() {
        let dom = parse();
        let parser = dom.parser();
        let scope: Vec<&tl::Node> = dom.nodes().iter().collect();
        assert_eq!(extract_title(&dom, parser).as_deref(), Some("Example Page"));

        let links = extract_links(&scope, parser, "https://example.com/index.html", 50);
        let hrefs: Vec<&str> = links.iter().filter_map(|l| l["href"].as_str()).collect();
        assert!(hrefs.contains(&"https://example.com/about"));
        assert!(hrefs.contains(&"https://example.com/contact"));
        assert!(hrefs.contains(&"https://example.com/skip"));
    }

    #[test]
    fn resolves_relative_urls() {
        assert_eq!(resolve_url("https://example.com/x/", "/contact"), "https://example.com/contact");
        assert_eq!(
            resolve_url("https://example.com/x/", "https://other.com/y"),
            "https://other.com/y"
        );
    }

    #[test]
    fn extracts_images_and_text() {
        let dom = parse();
        let parser = dom.parser();
        let scope: Vec<&tl::Node> = dom.nodes().iter().collect();
        let images = extract_images(&scope, "https://example.com/", 50);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0]["src"], "https://example.com/logo.png");
        assert_eq!(images[0]["alt"], "Logo");

        let text = extract_text(&scope, parser);
        assert!(text.contains("Hello World"));
        assert!(text.contains("Some bold text here"));
    }

    #[test]
    fn extracts_elements_filtered_by_selector() {
        let dom = parse();
        let parser = dom.parser();
        // Mirror the tool: collect handles within the matched subtree, then
        // resolve to nodes and serialize.
        let mut handles = Vec::new();
        for m in dom.query_selector("#main").unwrap() {
            collect_subtree_handles(m, parser, &mut handles);
        }
        let all_tags: Vec<&tl::Node> = handles.iter().filter_map(|h| h.get(parser)).collect();
        let elements = extract_elements(&all_tags, parser, 50);
        // #main plus its descendants: h1, p, b, 2 links, img, ...
        assert!(elements.len() >= 5);
        let tags: Vec<&str> = elements.iter().filter_map(|e| e["tag"].as_str()).collect();
        assert!(tags.contains(&"h1"));
        assert!(tags.contains(&"a"));
        assert!(tags.contains(&"img"));
    }

    #[test]
    fn parse_extract_defaults_and_custom() {
        assert_eq!(parse_extract(&json!({})).len(), 3);
        let kinds = parse_extract(&json!({ "extract": ["links", "images"] }));
        assert_eq!(kinds, vec![ExtractKind::Links, ExtractKind::Images]);
        let kinds = parse_extract(&json!({ "extract": "text, links" }));
        assert_eq!(kinds, vec![ExtractKind::Links, ExtractKind::Text]);
    }

    #[test]
    fn collapse_ws_normalizes_whitespace() {
        assert_eq!(collapse_ws("a   b\n\n  c"), "a b c");
        assert_eq!(collapse_ws("  trim  "), "trim");
    }

    #[tokio::test]
    async fn extracts_structured_data_from_live_server() {
        use httpmock::prelude::*;

        // Allow the local mock server (127.0.0.1) past the SSRF guard.
        crate::tools::web::common::ALLOW_PRIVATE_HOSTS.store(true, std::sync::atomic::Ordering::Relaxed);

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/page");
            then.status(200)
                .header("content-type", "text/html; charset=utf-8")
                .body(
                    r#"<!doctype html><html><head><title>Live</title></head>
                    <body><nav><a href="/skip">Skip</a></nav>
                    <main id="main">
                      <h1>Heading</h1>
                      <a href="/about">About</a>
                      <a href="https://ext.example/x">External</a>
                      <img src="/logo.png" alt="Logo" />
                    </main></body></html>"#,
                );
        });

        let url = format!("{}/page", server.base_url());
        let args = json!({ "url": url, "selector": "#main", "extract": ["links", "images", "text", "elements"] });
        let result = execute_web_extract(args, None).await.unwrap();

        let crate::types::ToolResultContent::Text(text) = &result.content[0] else {
            panic!("expected text content");
        };
        let value: Value = serde_json::from_str(&text.text).expect("result should be JSON");

        assert_eq!(value["title"], "Live");
        assert_eq!(value["url"], url);
        assert_eq!(value["selector"], "#main");

        let links = value["links"].as_array().expect("links array");
        let hrefs: Vec<String> = links
            .iter()
            .filter_map(|l| l["href"].as_str().map(str::to_string))
            .collect();
        assert!(hrefs.contains(&format!("{}/about", server.base_url())));
        assert!(hrefs.contains(&"https://ext.example/x".to_string()));
        // nav link is outside #main, so it must not appear.
        assert!(!hrefs.iter().any(|h| h.ends_with("/skip")));

        let images = value["images"].as_array().expect("images array");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0]["src"], format!("{}/logo.png", server.base_url()));

        let elements = value["elements"].as_array().expect("elements array");
        let tags: Vec<&str> = elements.iter().filter_map(|e| e["tag"].as_str()).collect();
        assert!(tags.contains(&"main"));
        assert!(tags.contains(&"a"));
        assert!(tags.contains(&"img"));
    }

    #[tokio::test]
    async fn rejects_missing_url() {
        let err = execute_web_extract(json!({}), None).await.unwrap_err();
        assert!(err.to_string().contains("Missing required argument: url"));
    }
}
