package websearch

import (
	"context"
	"fmt"
	"net/http"
)

func searchFirecrawl(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing FIRECRAWL_API_KEY")
	}
	var data struct {
		Success bool `json:"success"`
		Data    []struct {
			Title       string `json:"title"`
			URL         string `json:"url"`
			Description string `json:"description"`
		} `json:"data"`
	}
	err := doJSON(ctx, client, http.MethodPost, "https://api.firecrawl.dev/v1/search",
		map[string]string{"Authorization": "Bearer " + apiKey},
		map[string]any{"query": query, "limit": 10},
		&data)
	if err != nil {
		return nil, fmt.Errorf("firecrawl: %w", err)
	}
	if !data.Success {
		return nil, fmt.Errorf("firecrawl: search unsuccessful")
	}
	out := make([]Result, 0, len(data.Data))
	for _, item := range data.Data {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Description})
	}
	return out, nil
}
