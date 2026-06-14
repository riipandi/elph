package websearch

import (
	"context"
	"fmt"
	"net/http"
)

func searchJina(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	headers := map[string]string{"Accept": "application/json"}
	if apiKey != "" {
		headers["Authorization"] = "Bearer " + apiKey
	}
	var data struct {
		Data []struct {
			Title   string `json:"title"`
			URL     string `json:"url"`
			Snippet string `json:"snippet"`
		} `json:"data"`
	}
	err := doJSON(ctx, client, http.MethodGet, "https://s.jina.ai/"+urlQueryEscape(query), headers, nil, &data)
	if err != nil {
		return nil, fmt.Errorf("jina: %w", err)
	}
	out := make([]Result, 0, len(data.Data))
	for _, item := range data.Data {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Snippet})
	}
	return out, nil
}
