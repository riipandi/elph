// Package websearch implements reusable multi-engine web search with ENV-based
// provider selection and DuckDuckGo fallback. Engine ranking follows pi-extended/websearch.
package websearch

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"sort"
	"strings"
	"time"
)

// Result is a normalized search hit.
type Result struct {
	Title   string
	URL     string
	Snippet string
	Content string // Full page content (populated when include_content is enabled)
}

// EngineID identifies a search backend.
type EngineID string

const (
	EngineDuckDuckGo EngineID = "duckduckgo"
	EngineJina       EngineID = "jina"
	EngineBrave      EngineID = "brave"
	EngineSerpAPI    EngineID = "serpapi"
	EngineTavily     EngineID = "tavily"
	EngineFirecrawl  EngineID = "firecrawl"
	EnginePerplexity EngineID = "perplexity"
	EngineExa        EngineID = "exa"
)

// HTTPClient is the client used for outbound search requests. Tests may replace it.
var HTTPClient = &http.Client{Timeout: 20 * time.Second}

type engine struct {
	id          EngineID
	name        string
	rank        int
	requiresKey bool
	keyEnv      string
	search      searchFunc
}

type searchFunc func(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error)

var engines = []engine{
	{id: EngineDuckDuckGo, name: "DuckDuckGo", rank: 1, search: searchDuckDuckGo},
	{id: EngineJina, name: "Jina AI", rank: 2, keyEnv: "JINA_API_KEY", search: searchJina},
	{id: EngineBrave, name: "Brave Search", rank: 3, requiresKey: true, keyEnv: "BRAVE_SEARCH_API_KEY", search: searchBrave},
	{id: EngineSerpAPI, name: "SerpAPI", rank: 4, requiresKey: true, keyEnv: "SERPAPI_KEY", search: searchSerpAPI},
	{id: EngineTavily, name: "Tavily", rank: 5, requiresKey: true, keyEnv: "TAVILY_API_KEY", search: searchTavily},
	{id: EngineFirecrawl, name: "Firecrawl", rank: 6, requiresKey: true, keyEnv: "FIRECRAWL_API_KEY", search: searchFirecrawl},
	{id: EnginePerplexity, name: "Perplexity", rank: 7, requiresKey: true, keyEnv: "PERPLEXITY_API_KEY", search: searchPerplexity},
	{id: EngineExa, name: "Exa", rank: 8, requiresKey: true, keyEnv: "EXA_API_KEY", search: searchExa},
}

func engineByID(id EngineID) (engine, bool) {
	for _, e := range engines {
		if e.id == id {
			return e, true
		}
	}
	return engine{}, false
}

// NormalizeEngine maps aliases to canonical engine ids.
func NormalizeEngine(raw string) (EngineID, bool) {
	s := strings.ToLower(strings.TrimSpace(raw))
	switch s {
	case "", "auto":
		return "", true
	case "duckduckgo", "ddg":
		return EngineDuckDuckGo, true
	case "jina", "jina-search":
		return EngineJina, true
	case "brave", "brave-search":
		return EngineBrave, true
	case "serpapi", "serapi":
		return EngineSerpAPI, true
	case "tavily":
		return EngineTavily, true
	case "firecrawl":
		return EngineFirecrawl, true
	case "perplexity":
		return EnginePerplexity, true
	case "exa":
		return EngineExa, true
	default:
		return "", false
	}
}

func apiKeyFor(e engine) string {
	if e.keyEnv == "" {
		return ""
	}
	return strings.TrimSpace(os.Getenv(e.keyEnv))
}

// IsAvailable reports whether an engine can be used (API key present when required).
func IsAvailable(id EngineID) bool {
	e, ok := engineByID(id)
	if !ok {
		return false
	}
	if e.requiresKey && apiKeyFor(e) == "" {
		return false
	}
	return true
}

// Available returns configured engines sorted by rank (lowest first).
func Available() []EngineID {
	out := make([]EngineID, 0, len(engines))
	for _, e := range engines {
		if e.requiresKey && apiKeyFor(e) == "" {
			continue
		}
		out = append(out, e.id)
	}
	return out
}

func availableEngines() []engine {
	out := make([]engine, 0, len(engines))
	for _, e := range engines {
		if e.requiresKey && apiKeyFor(e) == "" {
			continue
		}
		out = append(out, e)
	}
	return out
}

// orderedTryList returns engines to attempt. Auto mode prefers the highest-ranked configured
// engine (best quality); DuckDuckGo is always tried last as fallback.
func orderedTryList(preferred EngineID) []engine {
	avail := availableEngines()
	if len(avail) == 0 {
		if e, ok := engineByID(EngineDuckDuckGo); ok {
			return []engine{e}
		}
		return nil
	}

	var ddg *engine
	var rest []engine
	for _, e := range avail {
		if e.id == EngineDuckDuckGo {
			copy := e
			ddg = &copy
			continue
		}
		rest = append(rest, e)
	}
	sort.Slice(rest, func(i, j int) bool { return rest[i].rank > rest[j].rank })

	if preferred != "" {
		var ordered []engine
		if e, ok := engineByID(preferred); ok && (!e.requiresKey || apiKeyFor(e) != "") {
			ordered = append(ordered, e)
		}
		for _, e := range rest {
			if e.id != preferred {
				ordered = append(ordered, e)
			}
		}
		if ddg != nil && preferred != EngineDuckDuckGo {
			ordered = append(ordered, *ddg)
		}
		return ordered
	}

	ordered := append([]engine(nil), rest...)
	if ddg != nil {
		ordered = append(ordered, *ddg)
	}
	return ordered
}

// SearchOption configures a web search call.
type SearchOption func(*searchOptions)

type searchOptions struct {
	engine         EngineID
	limit          int
	includeContent bool
}

// WithEngine sets the preferred search engine.
func WithEngine(engine string) SearchOption {
	return func(o *searchOptions) {
		if id, ok := NormalizeEngine(engine); ok {
			o.engine = id
		}
	}
}

// WithLimit sets the maximum number of results.
func WithLimit(n int) SearchOption {
	return func(o *searchOptions) {
		if n > 0 && n <= 20 {
			o.limit = n
		}
	}
}

// WithIncludeContent enables fetching full page content for each result.
func WithIncludeContent() SearchOption {
	return func(o *searchOptions) {
		o.includeContent = true
	}
}

// Search runs a web query. When engine is empty, auto-selects the lowest-ranked available engine.
// On failure, tries other engines and always falls back to DuckDuckGo last.
func Search(ctx context.Context, query string, opts ...SearchOption) (used EngineID, results []Result, err error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return "", nil, fmt.Errorf("empty search query")
	}

	var cfg searchOptions
	for _, opt := range opts {
		opt(&cfg)
	}

	// Apply defaults
	if cfg.limit <= 0 || cfg.limit > 20 {
		cfg.limit = 5
	}

	var preferred EngineID
	if cfg.engine != "" {
		preferred = cfg.engine
		if !IsAvailable(preferred) {
			e, _ := engineByID(preferred)
			if e.requiresKey {
				return "", nil, fmt.Errorf("%s requires %s", e.name, e.keyEnv)
			}
		}
	}

	var errs []string
	for _, e := range orderedTryList(preferred) {
		key := apiKeyFor(e)
		res, searchErr := e.search(ctx, HTTPClient, query, key)
		if searchErr == nil && len(res) > 0 {
			// Apply limit to results
			if len(res) > cfg.limit {
				res = res[:cfg.limit]
			}
			return e.id, res, nil
		}
		if searchErr != nil {
			errs = append(errs, fmt.Sprintf("%s: %v", e.name, searchErr))
		} else {
			errs = append(errs, fmt.Sprintf("%s: no results", e.name))
		}
	}

	return "", nil, fmt.Errorf("web search failed: %s", strings.Join(errs, "; "))
}

// Format renders search output for the WebSearch tool.
func Format(engine EngineID, query string, results []Result) string {
	var b strings.Builder
	fmt.Fprintf(&b, "engine: %s\nquery: %s\nresults: %d\n\n", engine, query, len(results))
	for i, r := range results {
		fmt.Fprintf(&b, "%d. %s\n", i+1, r.Title)
		fmt.Fprintf(&b, "   url: %s\n", r.URL)
		if r.Snippet != "" {
			fmt.Fprintf(&b, "   snippet: %s\n", r.Snippet)
		}
		if r.Content != "" {
			fmt.Fprintf(&b, "   content: %s\n", r.Content)
		}
		if i < len(results)-1 {
			b.WriteByte('\n')
		}
	}
	return strings.TrimRight(b.String(), "\n")
}
