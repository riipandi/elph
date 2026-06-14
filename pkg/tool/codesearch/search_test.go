package codesearch

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func clearCodeSearchEnv(t *testing.T) {
	t.Helper()
	for _, key := range []string{
		"GITHUB_PERSONAL_ACCESS_TOKEN", "GITHUB_TOKEN", "GH_TOKEN", "GITHUB_PAT", "GITHUB_HOST",
		"GITLAB_TOKEN", "GITLAB_PRIVATE_TOKEN", "PRIVATE_TOKEN",
		"GITLAB_HOST", "GITLAB_URL",
	} {
		t.Setenv(key, "")
	}
}

func TestGitHubTokenEnvPrecedence(t *testing.T) {
	clearCodeSearchEnv(t)
	t.Setenv("GITHUB_PERSONAL_ACCESS_TOKEN", "pat")
	t.Setenv("GITHUB_TOKEN", "github")
	t.Setenv("GH_TOKEN", "gh")
	require.Equal(t, "pat", GitHubToken())
}

func TestGitHubTokenOptional(t *testing.T) {
	clearCodeSearchEnv(t)
	require.Empty(t, GitHubToken())
	require.Equal(t, []ProviderID{ProviderGitHub}, Available())
}

func TestGitLabTokenEnv(t *testing.T) {
	clearCodeSearchEnv(t)
	t.Setenv("GITLAB_TOKEN", "gl")
	require.Equal(t, "gl", GitLabToken())
}

func TestSearchGitHubWithToken(t *testing.T) {
	clearCodeSearchEnv(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "Bearer gh-test", r.Header.Get("Authorization"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"items":[{"name":"main.go","path":"cmd/main.go","html_url":"https://github.com/o/r/blob/main.go","repository":{"full_name":"o/r"},"text_matches":[{"fragment":"func main"}]}]}`))
	}))
	defer srv.Close()

	t.Cleanup(SetSearchFuncsForTest(
		func(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
			require.Equal(t, "elph cli", query)
			require.Equal(t, "gh-test", token)
			return searchGitHubAt(ctx, client, srv.URL, query, token)
		},
		nil,
	))
	t.Setenv("GITHUB_TOKEN", "gh-test")

	used, results, err := Search(context.Background(), "elph cli", "")
	require.NoError(t, err)
	require.Equal(t, []ProviderID{ProviderGitHub}, used)
	require.Len(t, results, 1)
	require.Equal(t, "o/r", results[0].Repository)
}

func TestSearchGitHubAndGitLab(t *testing.T) {
	clearCodeSearchEnv(t)

	gh := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"items":[{"name":"a.go","path":"a.go","html_url":"https://gh","repository":{"full_name":"o/a"}}]}`))
	}))
	defer gh.Close()
	gl := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "gl-test", r.Header.Get("PRIVATE-TOKEN"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`[{"basename":"b.go","path":"b.go","project_id":1,"web_url":"https://gl"}]`))
	}))
	defer gl.Close()

	t.Cleanup(SetSearchFuncsForTest(
		func(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
			return searchGitHubAt(ctx, client, gh.URL, query, token)
		},
		func(ctx context.Context, client *http.Client, query, token, _ string) ([]Result, error) {
			return searchGitLabAt(ctx, client, gl.URL, query, token)
		},
	))

	t.Setenv("GITHUB_TOKEN", "gh-test")
	t.Setenv("GITLAB_TOKEN", "gl-test")

	used, results, err := Search(context.Background(), "query", "")
	require.NoError(t, err)
	require.Equal(t, []ProviderID{ProviderGitHub, ProviderGitLab}, used)
	require.Len(t, results, 2)
}

func TestSearchGitLabRequiresTokenWhenExplicit(t *testing.T) {
	clearCodeSearchEnv(t)
	_, _, err := Search(context.Background(), "x", "gitlab")
	require.Error(t, err)
	require.Contains(t, err.Error(), "GITLAB_TOKEN")
}

func TestSearchGitHubWithoutToken(t *testing.T) {
	clearCodeSearchEnv(t)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Empty(t, r.Header.Get("Authorization"))
		require.Equal(t, "application/vnd.github.text-match+json", r.Header.Get("Accept"))
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"items":[{"name":"x.go","path":"x.go","html_url":"https://x","repository":{"full_name":"o/r"}}]}`))
	}))
	defer srv.Close()

	t.Cleanup(SetSearchFuncsForTest(
		func(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
			require.Empty(t, token)
			return searchGitHubAt(ctx, client, srv.URL, query, token)
		},
		nil,
	))

	used, results, err := Search(context.Background(), "query", "")
	require.NoError(t, err)
	require.Equal(t, []ProviderID{ProviderGitHub}, used)
	require.Len(t, results, 1)
}

func TestSearchGitLabTokenStillQueriesGitHub(t *testing.T) {
	clearCodeSearchEnv(t)

	gh := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"items":[{"name":"a.go","path":"a.go","html_url":"https://gh","repository":{"full_name":"o/a"}}]}`))
	}))
	defer gh.Close()
	gl := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`[{"basename":"b.go","path":"b.go","project_id":1,"web_url":"https://gl"}]`))
	}))
	defer gl.Close()

	t.Cleanup(SetSearchFuncsForTest(
		func(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
			return searchGitHubAt(ctx, client, gh.URL, query, token)
		},
		func(ctx context.Context, client *http.Client, query, token, _ string) ([]Result, error) {
			return searchGitLabAt(ctx, client, gl.URL, query, token)
		},
	))
	t.Setenv("GITLAB_TOKEN", "gl-test")

	used, results, err := Search(context.Background(), "query", "")
	require.NoError(t, err)
	require.Equal(t, []ProviderID{ProviderGitHub, ProviderGitLab}, used)
	require.Len(t, results, 2)
}
