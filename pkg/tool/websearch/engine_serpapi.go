package websearch

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
)

func searchSerpAPI(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing SERPAPI_KEY")
	}
	u, _ := url.Parse("https://serpapi.com/search")
	q := u.Query()
	q.Set("q", query)
	q.Set("api_key", apiKey)
	q.Set("engine", "google")
	u.RawQuery = q.Encode()

	var data struct {
		OrganicResults []struct {
			Title   string `json:"title"`
			Link    string `json:"link"`
			Snippet string `json:"snippet"`
		} `json:"organic_results"`
	}
	err := doJSON(ctx, client, http.MethodGet, u.String(), nil, nil, &data)
	if err != nil {
		return nil, fmt.Errorf("serpapi: %w", err)
	}
	out := make([]Result, 0, len(data.OrganicResults))
	for _, item := range data.OrganicResults {
		out = append(out, Result{Title: item.Title, URL: item.Link, Snippet: item.Snippet})
	}
	return out, nil
}
