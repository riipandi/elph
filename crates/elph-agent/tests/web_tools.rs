//! Integration tests for websearch ranking and webfetch URL validation.

use elph_agent::WebSearchEngine;
use elph_agent::create_web_extract_tool;
use elph_agent::create_web_fetch_tool;
use elph_agent::create_web_search_tool;
use elph_agent::tools::web::engines::parse_ddg_html;
use elph_agent::tools::web::ranking::Engine;
use elph_agent::tools::web::ranking::ordered_try_list;

#[test]
fn ordered_try_list_explicit_engine_is_pinned() {
    // Explicit engine must not fall through to other providers (report 260809_0758).
    assert_eq!(ordered_try_list(Some(Engine::DuckDuckGo)), vec![Engine::DuckDuckGo]);
    assert_eq!(ordered_try_list(Some(Engine::Jina)), vec![Engine::Jina]);
    assert_eq!(ordered_try_list(Some(Engine::Exa)), vec![Engine::Exa]);
}

#[test]
fn ordered_try_list_auto_puts_duckduckgo_last() {
    let list = ordered_try_list(None);
    assert!(!list.is_empty());
    assert_eq!(*list.last().unwrap(), Engine::DuckDuckGo);
}

#[test]
fn parse_ddg_html_extracts_results() {
    let html = r#"<a class="result__a" href="https://go.dev">Go</a><a class="result__snippet">The Go language</a>"#;
    let results = parse_ddg_html(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Go");
    assert_eq!(results[0].url, "https://go.dev");
}

#[test]
fn web_tools_are_registered() {
    let search = create_web_search_tool();
    let fetch = create_web_fetch_tool();
    let extract = create_web_extract_tool();
    assert_eq!(search.tool.name, "web_search");
    assert_eq!(fetch.tool.name, "web_fetch");
    assert_eq!(extract.tool.name, "web_extract");
}

#[test]
fn engine_aliases_match_go_reference() {
    assert_eq!(WebSearchEngine::from_str_opt("serapi"), Some(Engine::Serpapi));
    assert_eq!(WebSearchEngine::from_str_opt("ddg"), Some(Engine::DuckDuckGo));
    assert!(Engine::Firecrawl.is_available());
    assert!(Engine::Jina.is_available());
}
