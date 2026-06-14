package websearch

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"regexp"
	"strings"
)

var jsonArrayRe = regexp.MustCompile(`\[[\s\S]*\]`)

func searchPerplexity(ctx context.Context, client *http.Client, query, apiKey string) ([]Result, error) {
	if apiKey == "" {
		return nil, fmt.Errorf("missing PERPLEXITY_API_KEY")
	}
	var data struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
			Citations []struct {
				URL string `json:"url"`
			} `json:"citations"`
		} `json:"choices"`
	}
	err := doJSON(ctx, client, http.MethodPost, "https://api.perplexity.ai/chat/completions",
		map[string]string{"Authorization": "Bearer " + apiKey},
		map[string]any{
			"model": "sonar",
			"messages": []map[string]string{
				{
					"role":    "system",
					"content": "You are a search assistant. Return search results as a JSON array with objects containing title, url, and snippet fields.",
				},
				{"role": "user", "content": query},
			},
		}, &data)
	if err != nil {
		return nil, fmt.Errorf("perplexity: %w", err)
	}
	if len(data.Choices) == 0 {
		return nil, fmt.Errorf("perplexity: empty response")
	}
	content := data.Choices[0].Message.Content
	if m := jsonArrayRe.FindString(content); m != "" {
		var parsed []Result
		if json.Unmarshal([]byte(m), &parsed) == nil && len(parsed) > 0 {
			return parsed, nil
		}
	}
	citations := data.Choices[0].Citations
	out := make([]Result, 0, len(citations))
	for i, c := range citations {
		out = append(out, Result{
			Title:   fmt.Sprintf("Result %d", i+1),
			URL:     c.URL,
			Snippet: strings.TrimSpace(content),
		})
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("perplexity: no parseable results")
	}
	return out, nil
}
