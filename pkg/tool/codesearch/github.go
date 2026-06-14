package codesearch

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

func searchGitHub(ctx context.Context, client *http.Client, query, token string) ([]Result, error) {
	base := strings.TrimRight(GitHubRESTBase(), "/")
	return searchGitHubAt(ctx, client, base+"/search/code", query, token)
}

func searchGitHubAt(ctx context.Context, client *http.Client, endpoint, query, token string) ([]Result, error) {
	u, err := url.Parse(endpoint)
	if err != nil {
		return nil, err
	}
	if u.RawQuery == "" {
		q := u.Query()
		q.Set("q", query)
		q.Set("per_page", "20")
		u.RawQuery = q.Encode()
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/vnd.github.text-match+json")
	req.Header.Set("X-GitHub-Api-Version", "2022-11-28")
	req.Header.Set("User-Agent", "Elph/1.0 (+https://github.com/riipandi/elph)")
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}

	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		msg := trimAPIError(data)
		if resp.StatusCode == http.StatusUnauthorized && token == "" {
			msg += " (set GITHUB_PERSONAL_ACCESS_TOKEN to authenticate — token is optional but required by the GitHub code search API)"
		}
		return nil, fmt.Errorf("status %s: %s", resp.Status, msg)
	}

	var payload struct {
		Items []struct {
			Name       string `json:"name"`
			Path       string `json:"path"`
			HTMLURL    string `json:"html_url"`
			Repository struct {
				FullName string `json:"full_name"`
			} `json:"repository"`
			TextMatches []struct {
				Fragment string `json:"fragment"`
			} `json:"text_matches"`
		} `json:"items"`
	}
	if err := json.Unmarshal(data, &payload); err != nil {
		return nil, err
	}

	out := make([]Result, 0, len(payload.Items))
	for _, item := range payload.Items {
		snippet := ""
		if len(item.TextMatches) > 0 {
			snippet = strings.TrimSpace(item.TextMatches[0].Fragment)
		}
		path := item.Path
		if path == "" {
			path = item.Name
		}
		out = append(out, Result{
			Repository: item.Repository.FullName,
			Path:       path,
			URL:        item.HTMLURL,
			Snippet:    snippet,
		})
	}
	return out, nil
}
