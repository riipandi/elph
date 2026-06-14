package runtime

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/riipandi/elph/pkg/tool/websearch"
	"github.com/stretchr/testify/require"
)

func TestExecuteWebSearch(t *testing.T) {
	t.Parallel()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`<a class="result__a" href="https://elph.dev">Elph</a><a class="result__snippet">Agent CLI</a>`))
	}))
	defer srv.Close()

	t.Cleanup(websearch.ResetEnginesForTest)
	websearch.SetEnginesForTest([]websearch.EngineDef{
		{
			ID: websearch.EngineDuckDuckGo, Name: "DuckDuckGo", Rank: 1,
			Search: websearch.MockDuckDuckGoAt(srv.URL),
		},
	})

	result := ExecuteTool(context.Background(), t.TempDir(), tool.WebSearch, map[string]any{
		"query": "elph agent",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "engine: duckduckgo")
	require.Contains(t, result.Output, "https://elph.dev")
}

func TestExecuteWebSearchMissingQuery(t *testing.T) {
	t.Parallel()

	result := ExecuteTool(context.Background(), t.TempDir(), tool.WebSearch, map[string]any{})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "query")
}
