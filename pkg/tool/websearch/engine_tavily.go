package websearch

import (
	"context"
	"fmt"
	"net/http"
)

func searchTavily(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing TAVILY_API_KEY")
	}
	var data struct {
		Results []struct {
			Title   string `json:"title"`
			URL     string `json:"url"`
			Content string `json:"content"`
		} `json:"results"`
	}
	err := doJSON(ctx, client, http.MethodPost, "https://api.tavily.com/search", nil, map[string]any{
		"api_key":             apiKey,
		"query":               query,
		"search_depth":        "basic",
		"include_answer":      false,
		"include_raw_content": false,
	}, &data)
	if err != nil {
		return nil, fmt.Errorf("tavily: %w", err)
	}
	out := make([]Result, 0, len(data.Results))
	for _, item := range data.Results {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Content})
	}
	return out, nil
}
