package websearch

import (
	"context"
	"fmt"
	"net/http"
)

func searchBrave(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing BRAVE_SEARCH_API_KEY")
	}
	var data struct {
		Web struct {
			Results []struct {
				Title       string `json:"title"`
				URL         string `json:"url"`
				Description string `json:"description"`
			} `json:"results"`
		} `json:"web"`
	}
	err := doJSON(ctx, client, http.MethodGet,
		"https://api.search.brave.com/res/v1/web/search?q="+urlQueryEscape(query),
		map[string]string{
			"Accept":               "application/json",
			"X-Subscription-Token": apiKey,
		}, nil, &data)
	if err != nil {
		return nil, fmt.Errorf("brave: %w", err)
	}
	out := make([]Result, 0, len(data.Web.Results))
	for _, item := range data.Web.Results {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Description})
	}
	return out, nil
}
