package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestOpenAICompatibleComplete(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/chat/completions", r.URL.Path)
		require.Equal(t, "Bearer test-key", r.Header.Get("Authorization"))

		_ = json.NewEncoder(w).Encode(map[string]any{
			"choices": []map[string]any{{
				"message": map[string]string{"content": "hello from gpt"},
			}},
		})
	}))
	defer srv.Close()

	p := NewOpenAICompatible(IDOpenAI, "test-key", srv.URL, "gpt-test")

	got, err := p.Complete(context.Background(), TurnRequest{
		SystemPrompt: "sys",
		UserPrompt:   "hi",
		Model:        "gpt-test",
	})
	require.NoError(t, err)
	require.Equal(t, "hello from gpt", got)
}