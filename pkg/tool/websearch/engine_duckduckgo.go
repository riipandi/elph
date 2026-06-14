package websearch

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"regexp"
)

var (
	ddgLinkRe    = regexp.MustCompile(`<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>([\s\S]*?)</a>`)
	ddgSnippetRe = regexp.MustCompile(`<a[^>]*class="result__snippet"[^>]*>([\s\S]*?)</a>`)
)

func searchDuckDuckGo(ctx context.Context, client *http.Client, query, _ string) ([]Result, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet,
		"https://html.duckduckgo.com/html/?q="+urlQueryEscape(query), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")

	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("status %s", resp.Status)
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, 2<<20))
	if err != nil {
		return nil, err
	}
	return parseDDGResults(string(body)), nil
}

func parseDDGResults(html string) []Result {
	links := ddgLinkRe.FindAllStringSubmatch(html, -1)
	snippets := ddgSnippetRe.FindAllStringSubmatch(html, -1)
	n := len(links)
	if len(snippets) < n {
		n = len(snippets)
	}
	out := make([]Result, 0, n)
	for i := 0; i < n; i++ {
		out = append(out, Result{
			Title:   stripHTML(links[i][2]),
			URL:     links[i][1],
			Snippet: stripHTML(snippets[i][1]),
		})
	}
	return out
}
