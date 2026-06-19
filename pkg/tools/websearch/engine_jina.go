package websearch

import (
	"context"
	"fmt"

	"resty.dev/v3"
)

func searchJina(ctx context.Context, client *resty.Client, query, apiKey string) ([]Result, error) {
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
	err := doJSON(ctx, client, "GET", "https://s.jina.ai/"+urlQueryEscape(query), headers, nil, &data)
	if err != nil {
		return nil, fmt.Errorf("jina: %w", err)
	}
	out := make([]Result, 0, len(data.Data))
	for _, item := range data.Data {
		out = append(out, Result{Title: item.Title, URL: item.URL, Snippet: item.Snippet})
	}
	return out, nil
}
