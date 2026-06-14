package runtime

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/riipandi/elph/pkg/tool/codesearch"
	"github.com/riipandi/elph/pkg/tool/fetchurl"
	"github.com/stretchr/testify/require"
)

func TestExecuteFetchURL(t *testing.T) {
	t.Parallel()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/plain")
		_, _ = w.Write([]byte("plain body"))
	}))
	defer srv.Close()

	fetchurl.SetAllowPrivateHostsForTest(true)
	t.Cleanup(func() { fetchurl.SetAllowPrivateHostsForTest(false) })
	orig := fetchurl.HTTPClient
	t.Cleanup(func() { fetchurl.HTTPClient = orig })
	fetchurl.HTTPClient = srv.Client()

	result := ExecuteTool(context.Background(), t.TempDir(), tool.FetchURL, map[string]any{
		"url": srv.URL,
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "plain body")
}

func TestExecuteCodeSearch(t *testing.T) {
	t.Parallel()

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"items":[{"name":"x.go","path":"x.go","html_url":"https://x","repository":{"full_name":"o/r"}}]}`))
	}))
	defer srv.Close()

	t.Cleanup(codesearch.SetSearchFuncsForTest(
		func(ctx context.Context, client *http.Client, query, _ string) ([]codesearch.Result, error) {
			return codesearch.SearchGitHubAt(ctx, client, srv.URL, query, "")
		},
		nil,
	))

	result := ExecuteTool(context.Background(), t.TempDir(), tool.CodeSearch, map[string]any{
		"query": "fmt.Println",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "providers: github")
	require.Contains(t, result.Output, "o/r")
}
