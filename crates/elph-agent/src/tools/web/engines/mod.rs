//! Search engine HTTP backends — one module per provider.

pub mod brave;
pub mod duckduckgo;
pub mod exa;
pub mod firecrawl;
pub mod jina;
pub mod perplexity;
pub mod serpapi;
pub mod tavily;

use reqwest::Client;

use super::ranking::{Engine, SearchResult};

pub use duckduckgo::parse_ddg_html;

/// Dispatch a search query to the appropriate engine backend.
///
/// Each engine is treated as a separate provider for resilience (rate limiting,
/// circuit breaking). The provider_id is the engine name in lowercase.
pub async fn search_engine(
    client: &Client,
    engine: Engine,
    query: &str,
    api_key: &str,
) -> anyhow::Result<Vec<SearchResult>> {
    let provider_id = engine.name().to_lowercase().replace(' ', "-");

    // Check resilience before calling the engine
    elph_ai::resilience::check_provider_resilience(&provider_id)?;
    log::debug!("web search start engine={provider_id}");

    let result = match engine {
        Engine::DuckDuckGo => duckduckgo::search(client, query).await,
        Engine::Brave => brave::search(client, query, api_key).await,
        Engine::Exa => exa::search(client, query, api_key).await,
        Engine::Firecrawl => firecrawl::search(client, query, api_key).await,
        Engine::Jina => jina::search(client, query, api_key).await,
        Engine::Perplexity => perplexity::search(client, query, api_key).await,
        Engine::Tavily => tavily::search(client, query, api_key).await,
        Engine::Serpapi => serpapi::search(client, query, api_key).await,
    };

    match &result {
        Ok(hits) => {
            log::debug!("web search ok engine={provider_id} hits={}", hits.len());
            elph_ai::resilience::record_provider_success(&provider_id);
        }
        Err(e) => {
            log::warn!("web search failed engine={provider_id}: {e:#}");
            let msg = e.to_string();
            // Record failure for server errors and rate limits
            if msg.contains("429")
                || msg.contains("500")
                || msg.contains("502")
                || msg.contains("503")
                || msg.contains("504")
                || msg.contains("connection")
                || msg.contains("timeout")
            {
                elph_ai::resilience::record_provider_failure(&provider_id);
            }
        }
    }

    result
}
