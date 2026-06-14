// Package codesearch searches code on GitHub and GitLab.
// GitHub token (GITHUB_PERSONAL_ACCESS_TOKEN) is optional; GitLab requires GITLAB_TOKEN.
package codesearch

import (
	"context"
	"fmt"
	"net/http"
	"strings"
	"time"
)

// ProviderID identifies a code search backend.
type ProviderID string

const (
	ProviderGitHub ProviderID = "github"
	ProviderGitLab ProviderID = "gitlab"
)

// Result is a normalized code search hit.
type Result struct {
	Repository string
	Path       string
	URL        string
	Snippet    string
}

// HTTPClient is used for outbound API requests. Tests may replace it.
var HTTPClient = &http.Client{Timeout: 20 * time.Second}

// Available returns providers that CodeSearch may use.
func Available() []ProviderID {
	out := []ProviderID{ProviderGitHub}
	if GitLabToken() != "" {
		out = append(out, ProviderGitLab)
	}
	return out
}

// NormalizeProvider maps aliases to canonical provider ids.
func NormalizeProvider(raw string) (ProviderID, bool) {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "", "auto":
		return "", true
	case "github", "gh":
		return ProviderGitHub, true
	case "gitlab", "gl":
		return ProviderGitLab, true
	default:
		return "", false
	}
}

// Search runs a code query across configured providers.
func Search(ctx context.Context, query, provider string) (used []ProviderID, results []Result, err error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, nil, fmt.Errorf("empty search query")
	}

	var preferred ProviderID
	if strings.TrimSpace(provider) != "" {
		id, ok := NormalizeProvider(provider)
		if !ok {
			return nil, nil, fmt.Errorf("unknown provider: %s", provider)
		}
		preferred = id
	}

	providers := orderedProviders(preferred)
	if len(providers) == 0 {
		return nil, nil, fmt.Errorf("no code search provider available — set GITLAB_TOKEN for GitLab search")
	}

	var errs []string
	var all []Result
	var usedIDs []ProviderID

	for _, p := range providers {
		res, searchErr := searchProvider(ctx, HTTPClient, p, query)
		if searchErr != nil {
			errs = append(errs, fmt.Sprintf("%s: %v", p, searchErr))
			continue
		}
		if len(res) == 0 {
			errs = append(errs, fmt.Sprintf("%s: no results", p))
			continue
		}
		usedIDs = append(usedIDs, p)
		all = append(all, res...)
	}

	if len(all) == 0 {
		return nil, nil, fmt.Errorf("code search failed: %s", strings.Join(errs, "; "))
	}
	return usedIDs, all, nil
}

func orderedProviders(preferred ProviderID) []ProviderID {
	hasGL := GitLabToken() != ""

	var out []ProviderID
	add := func(id ProviderID) {
		for _, existing := range out {
			if existing == id {
				return
			}
		}
		out = append(out, id)
	}

	if preferred == ProviderGitHub {
		add(ProviderGitHub)
		if hasGL {
			add(ProviderGitLab)
		}
		return out
	}
	if preferred == ProviderGitLab {
		if !hasGL {
			return nil
		}
		add(ProviderGitLab)
		add(ProviderGitHub)
		return out
	}

	add(ProviderGitHub)
	if hasGL {
		add(ProviderGitLab)
	}
	return out
}

func searchProvider(ctx context.Context, client *http.Client, p ProviderID, query string) ([]Result, error) {
	switch p {
	case ProviderGitHub:
		return searchGitHubDispatch(ctx, client, query, GitHubToken())
	case ProviderGitLab:
		token := GitLabToken()
		if token == "" {
			return nil, fmt.Errorf("GITLAB_TOKEN is not set")
		}
		return searchGitLabDispatch(ctx, client, query, token, GitLabAPIBase())
	default:
		return nil, fmt.Errorf("unsupported provider")
	}
}

// Format renders code search output for the CodeSearch tool.
func Format(providers []ProviderID, query string, results []Result) string {
	var b strings.Builder
	fmt.Fprintf(&b, "providers: %s\nquery: %s\nresults: %d\n\n", joinProviders(providers), query, len(results))
	for i, r := range results {
		repo := r.Repository
		if repo == "" {
			repo = "(unknown)"
		}
		fmt.Fprintf(&b, "%d. %s — %s\n", i+1, repo, r.Path)
		if r.URL != "" {
			fmt.Fprintf(&b, "   url: %s\n", r.URL)
		}
		if r.Snippet != "" {
			fmt.Fprintf(&b, "   snippet: %s\n", r.Snippet)
		}
		if i < len(results)-1 {
			b.WriteByte('\n')
		}
	}
	return strings.TrimRight(b.String(), "\n")
}

func joinProviders(ids []ProviderID) string {
	if len(ids) == 0 {
		return ""
	}
	parts := make([]string, len(ids))
	for i, id := range ids {
		parts[i] = string(id)
	}
	return strings.Join(parts, ", ")
}
