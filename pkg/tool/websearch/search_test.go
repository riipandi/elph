package websearch

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizeEngine(t *testing.T) {
	t.Parallel()

	id, ok := NormalizeEngine("serapi")
	require.True(t, ok)
	require.Equal(t, EngineSerpAPI, id)

	_, ok = NormalizeEngine("nope")
	require.False(t, ok)
}

func TestParseDDGResults(t *testing.T) {
	t.Parallel()

	html := `<a class="result__a" href="https://example.com">Example <b>Site</b></a>
<a class="result__snippet">A short <i>snippet</i> here</a>`
	results := parseDDGResults(html)
	require.Len(t, results, 1)
	require.Equal(t, "Example Site", results[0].Title)
	require.Equal(t, "https://example.com", results[0].URL)
	require.Equal(t, "A short snippet here", results[0].Snippet)
}

func TestOrderedTryListAutoPrefersConfiguredEngine(t *testing.T) {
	clearSearchAPIKeys(t)
	t.Setenv("TAVILY_API_KEY", "tv-test")

	list := orderedTryList("")
	require.GreaterOrEqual(t, len(list), 2)
	require.Equal(t, EngineTavily, list[0].id)
	require.Equal(t, EngineDuckDuckGo, list[len(list)-1].id)
}

func TestOrderedTryListExplicitEngine(t *testing.T) {
	list := orderedTryList(EngineJina)
	require.NotEmpty(t, list)
	require.Equal(t, EngineJina, list[0].id)
	require.Equal(t, EngineDuckDuckGo, list[len(list)-1].id)
}

func TestSearchFallbackToDuckDuckGo(t *testing.T) {
	clearSearchAPIKeys(t)
	ddg := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`<a class="result__a" href="https://go.dev">Go</a><a class="result__snippet">The Go language</a>`))
	}))
	defer ddg.Close()

	t.Cleanup(ResetEnginesForTest)
	SetEnginesForTest([]EngineDef{
		{
			ID: EngineTavily, Name: "Tavily", Rank: 5, RequiresKey: true, KeyEnv: "TAVILY_API_KEY",
			Search: func(context.Context, *http.Client, string, string) ([]Result, error) {
				return nil, errTest("tavily down")
			},
		},
		{ID: EngineDuckDuckGo, Name: "DuckDuckGo", Rank: 1, Search: MockDuckDuckGoAt(ddg.URL)},
	})
	t.Setenv("TAVILY_API_KEY", "tv-test")

	used, results, err := Search(context.Background(), "golang", "")
	require.NoError(t, err)
	require.Equal(t, EngineDuckDuckGo, used)
	require.Len(t, results, 1)
	require.Equal(t, "Go", results[0].Title)
}

func TestSearchRequiresKeyForExplicitEngine(t *testing.T) {
	clearSearchAPIKeys(t)
	_, _, err := Search(context.Background(), "test", "tavily")
	require.Error(t, err)
	require.Contains(t, err.Error(), "TAVILY_API_KEY")
}

func TestSearchTavilyViaMockServer(t *testing.T) {
	clearSearchAPIKeys(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, http.MethodPost, r.Method)
		require.Equal(t, "/search", r.URL.Path)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"results":[{"title":"Hit","url":"https://hit.test","content":"snippet"}]}`))
	}))
	defer srv.Close()

	tavily := func(ctx context.Context, _ *http.Client, query, apiKey string) ([]Result, error) {
		require.Equal(t, "elph", query)
		require.Equal(t, "tv-key", apiKey)
		return searchTavilyAt(ctx, HTTPClient, srv.URL+"/search", query, apiKey)
	}

	t.Cleanup(ResetEnginesForTest)
	SetEnginesForTest([]EngineDef{
		{ID: EngineTavily, Name: "Tavily", Rank: 5, RequiresKey: true, KeyEnv: "TAVILY_API_KEY", Search: tavily},
		{ID: EngineDuckDuckGo, Name: "DuckDuckGo", Rank: 1, Search: searchDuckDuckGo},
	})
	t.Setenv("TAVILY_API_KEY", "tv-key")

	used, results, err := Search(context.Background(), "elph", "")
	require.NoError(t, err)
	require.Equal(t, EngineTavily, used)
	require.Len(t, results, 1)
	require.Equal(t, "https://hit.test", results[0].URL)
}

type errTest string

func (e errTest) Error() string { return string(e) }

func searchTavilyAt(ctx context.Context, client *http.Client, endpoint, query, apiKey string) ([]Result, error) {
	var data struct {
		Results []struct {
			Title   string `json:"title"`
			URL     string `json:"url"`
			Content string `json:"content"`
		} `json:"results"`
	}
	err := doJSON(ctx, client, http.MethodPost, endpoint, nil, map[string]any{
		"api_key": apiKey, "query": query,
	}, &data)
	if err != nil {
		return nil, err
	}
	out := make([]Result, 0, len(data.Results))
	for _, item := range data.Results {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Content})
	}
	return out, nil
}

func clearSearchAPIKeys(t *testing.T) {
	t.Helper()
	for _, key := range []string{
		"JINA_API_KEY", "BRAVE_SEARCH_API_KEY", "SERPAPI_KEY", "TAVILY_API_KEY",
		"FIRECRAWL_API_KEY", "PERPLEXITY_API_KEY", "EXA_API_KEY",
	} {
		t.Setenv(key, "")
	}
}

func TestAvailableRespectsEnv(t *testing.T) {
	clearSearchAPIKeys(t)
	t.Setenv("EXA_API_KEY", "exa")
	avail := Available()
	require.Contains(t, avail, EngineDuckDuckGo)
	require.Contains(t, avail, EngineJina)
	require.Contains(t, avail, EngineExa)
	require.NotContains(t, avail, EngineTavily)
}

func TestFormatOutput(t *testing.T) {
	out := Format(EngineJina, "go modules", []Result{{Title: "Go Modules", URL: "https://go.dev", Snippet: "docs"}})
	require.Contains(t, out, "engine: jina")
	require.Contains(t, out, "query: go modules")
	require.Contains(t, out, "url: https://go.dev")
}

func TestMain(m *testing.M) {
	HTTPClient = &http.Client{}
	os.Exit(m.Run())
}
