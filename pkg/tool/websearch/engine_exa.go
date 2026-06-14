package websearch

import (
	"context"
	"fmt"
	"net/http"
	"strings"
)

func searchExa(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing EXA_API_KEY")
	}
	var data struct {
		Results []struct {
			Title         string   `json:"title"`
			URL           string   `json:"url"`
			Highlights    []string `json:"highlights"`
			Text          string   `json:"text"`
			Summary       string   `json:"summary"`
			PublishedDate string   `json:"publishedDate"`
			Author        string   `json:"author"`
		} `json:"results"`
	}
	err := doJSON(ctx, client, http.MethodPost, "https://api.exa.ai/search",
		map[string]string{"x-api-key": apiKey},
		map[string]any{
			"query":      query,
			"numResults": 10,
			"contents":   map[string]any{"highlights": true},
		}, &data)
	if err != nil {
		return nil, fmt.Errorf("exa: %w", err)
	}
	out := make([]Result, 0, len(data.Results))
	for _, item := range data.Results {
		title := item.Title
		var meta []string
		if item.PublishedDate != "" {
			meta = append(meta, item.PublishedDate[:min(10, len(item.PublishedDate))])
		}
		if item.Author != "" {
			meta = append(meta, item.Author)
		}
		if len(meta) > 0 {
			title += " (" + strings.Join(meta, " · ") + ")"
		}
		snippet := ""
		if len(item.Highlights) > 0 {
			snippet = item.Highlights[0]
		} else if item.Text != "" {
			snippet = item.Text
		}
		if len(snippet) > 300 {
			snippet = snippet[:300]
		}
		out = append(out, Result{Title: title, URL: item.URL, Snippet: snippet})
	}
	return out, nil
}
