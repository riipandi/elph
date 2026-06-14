package codesearch

import (
	"context"
	"net/http"
)

type githubSearchFunc func(context.Context, *http.Client, string, string) ([]Result, error)
type gitlabSearchFunc func(context.Context, *http.Client, string, string, string) ([]Result, error)

var (
	githubSearchFn = searchGitHub
	gitlabSearchFn = searchGitLab
)

func searchGitHubDispatch(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
	return githubSearchFn(ctx, client, query, token)
}

func searchGitLabDispatch(ctx context.Context, client *http.Client, query, token, apiBase string) ([]Result, error) {
	return gitlabSearchFn(ctx, client, query, token, apiBase)
}

// SetSearchFuncsForTest replaces provider search functions for the duration of a test.
func SetSearchFuncsForTest(gh githubSearchFunc, gl gitlabSearchFunc) func() {
	prevGH, prevGL := githubSearchFn, gitlabSearchFn
	if gh != nil {
		githubSearchFn = gh
	}
	if gl != nil {
		gitlabSearchFn = gl
	}
	return func() {
		githubSearchFn = prevGH
		gitlabSearchFn = prevGL
	}
}

// SearchGitHubAt calls a GitHub code search endpoint (for tests in other packages).
func SearchGitHubAt(ctx context.Context, client *http.Client, endpoint, query, token string) ([]Result, error) {
	return searchGitHubAt(ctx, client, endpoint, query, token)
}

// SearchGitLabAt calls a GitLab code search endpoint (for tests in other packages).
func SearchGitLabAt(ctx context.Context, client *http.Client, endpoint, query, token string) ([]Result, error) {
	return searchGitLabAt(ctx, client, endpoint, query, token)
}
