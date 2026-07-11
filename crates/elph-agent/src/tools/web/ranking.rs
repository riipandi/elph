//! Search engine ranking and availability.

use std::env;

use serde::{Deserialize, Serialize};

/// Normalized search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Search engine identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    DuckDuckGo,
    Brave,
    Exa,
    Firecrawl,
    Jina,
    Perplexity,
    Tavily,
    Serpapi,
}

impl Engine {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "" | "auto" => None,
            "duckduckgo" | "ddg" => Some(Self::DuckDuckGo),
            "brave" | "brave-search" => Some(Self::Brave),
            "exa" => Some(Self::Exa),
            "firecrawl" => Some(Self::Firecrawl),
            "jina" | "jina-search" => Some(Self::Jina),
            "perplexity" => Some(Self::Perplexity),
            "tavily" => Some(Self::Tavily),
            "serpapi" | "serapi" => Some(Self::Serpapi),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Brave => "Brave Search",
            Self::Exa => "Exa",
            Self::Firecrawl => "Firecrawl",
            Self::Jina => "Jina AI",
            Self::Perplexity => "Perplexity",
            Self::Tavily => "Tavily",
            Self::Serpapi => "SerpAPI",
        }
    }

    /// Lower rank = listed earlier in the registry (DuckDuckGo is the baseline fallback).
    pub fn rank(&self) -> u8 {
        match self {
            Self::DuckDuckGo => 1,
            Self::Jina => 2,
            Self::Brave => 3,
            Self::Serpapi => 4,
            Self::Tavily => 5,
            Self::Firecrawl => 6,
            Self::Perplexity => 7,
            Self::Exa => 8,
        }
    }

    pub fn key_env(&self) -> Option<&'static str> {
        match self {
            Self::DuckDuckGo => None,
            Self::Brave => Some("BRAVE_SEARCH_API_KEY"),
            Self::Exa => Some("EXA_API_KEY"),
            Self::Firecrawl => Some("FIRECRAWL_API_KEY"),
            Self::Jina => Some("JINA_API_KEY"),
            Self::Perplexity => Some("PERPLEXITY_API_KEY"),
            Self::Tavily => Some("TAVILY_API_KEY"),
            Self::Serpapi => Some("SERPAPI_KEY"),
        }
    }

    /// Engines that are skipped when their API key env var is unset.
    pub fn requires_key(&self) -> bool {
        !matches!(self, Self::DuckDuckGo | Self::Jina | Self::Firecrawl)
    }

    pub fn api_key(&self) -> String {
        self.key_env()
            .and_then(|env_var| env::var(env_var).ok())
            .map(|k| k.trim().to_string())
            .unwrap_or_default()
    }

    pub fn is_available(&self) -> bool {
        if !self.requires_key() {
            return true;
        }
        !self.api_key().is_empty()
    }
}

const ALL_ENGINES: [Engine; 8] = [
    Engine::DuckDuckGo,
    Engine::Jina,
    Engine::Brave,
    Engine::Serpapi,
    Engine::Tavily,
    Engine::Firecrawl,
    Engine::Perplexity,
    Engine::Exa,
];

pub fn available_engines() -> Vec<Engine> {
    let mut engines: Vec<Engine> = ALL_ENGINES.into_iter().filter(|e| e.is_available()).collect();
    engines.sort_by_key(|e| e.rank());
    engines
}

/// Auto mode prefers the highest-ranked configured engine; DuckDuckGo is always tried last.
pub fn ordered_try_list(preferred: Option<Engine>) -> Vec<Engine> {
    let avail = available_engines();
    if avail.is_empty() {
        return vec![Engine::DuckDuckGo];
    }

    let mut ddg = None;
    let mut rest = Vec::new();
    for engine in avail {
        if engine == Engine::DuckDuckGo {
            ddg = Some(engine);
        } else {
            rest.push(engine);
        }
    }
    rest.sort_by_key(|e| std::cmp::Reverse(e.rank()));

    let mut ordered = Vec::new();
    if let Some(pref) = preferred {
        if pref.is_available() {
            ordered.push(pref);
        }
        for engine in rest {
            if engine != pref {
                ordered.push(engine);
            }
        }
        if ddg.is_some() && pref != Engine::DuckDuckGo {
            ordered.push(Engine::DuckDuckGo);
        }
    } else {
        ordered.extend(rest);
        if ddg.is_some() {
            ordered.push(Engine::DuckDuckGo);
        }
    }
    ordered
}

pub fn format_results(engine: Engine, query: &str, results: &[SearchResult]) -> String {
    let mut output = format!(
        "engine: {}\nquery: {}\nresults: {}\n\n",
        engine.name().to_lowercase(),
        query,
        results.len()
    );
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", i + 1, result.title));
        output.push_str(&format!("   url: {}\n", result.url));
        if !result.snippet.is_empty() {
            output.push_str(&format!("   snippet: {}\n", result.snippet));
        }
        if let Some(content) = &result.content {
            output.push_str(&format!("   content: {}\n", content));
        }
        if i < results.len() - 1 {
            output.push('\n');
        }
    }
    output.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_aliases() {
        assert_eq!(Engine::from_str_opt(""), None);
        assert_eq!(Engine::from_str_opt("auto"), None);
        assert_eq!(Engine::from_str_opt("ddg"), Some(Engine::DuckDuckGo));
        assert_eq!(Engine::from_str_opt("serapi"), Some(Engine::Serpapi));
        assert_eq!(Engine::from_str_opt("nope"), None);
    }

    #[test]
    fn ranking_order() {
        let mut engines = vec![Engine::Exa, Engine::DuckDuckGo, Engine::Brave, Engine::Tavily];
        engines.sort_by_key(|e| e.rank());
        assert_eq!(engines, vec![Engine::DuckDuckGo, Engine::Brave, Engine::Tavily, Engine::Exa]);
    }
}
