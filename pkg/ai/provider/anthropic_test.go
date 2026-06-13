package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestAnthropicComplete(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/v1/messages", r.URL.Path)
		require.Equal(t, "test-key", r.Header.Get("x-api-key"))
		require.Equal(t, "custom", r.Header.Get("x-custom"))

		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		require.Equal(t, "sys", body["system"])
		require.Equal(t, 0.4, body["temperature"])

		_ = json.NewEncoder(w).Encode(map[string]any{
			"content": []map[string]string{{"type": "text", "text": "hello from claude"}},
		})
	}))
	defer srv.Close()

	p := NewAnthropic(AnthropicOptions{
		ID:          "anthropic",
		APIKey:      "test-key",
		Model:       "claude-test",
		BaseURL:     srv.URL + "/v1",
		Headers:     map[string]string{"x-custom": "custom"},
		MaxTokens:   1024,
		Temperature: 0.4,
	})

	got, err := p.Complete(context.Background(), TurnRequest{
		SystemPrompt: "sys",
		UserPrompt:   "hi",
		Model:        "claude-test",
	})
	require.NoError(t, err)
	require.Equal(t, "hello from claude", got)
}
