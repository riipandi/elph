package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewProviderOpenAI(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "Bearer secret", r.Header.Get("Authorization"))
		_ = json.NewEncoder(w).Encode(map[string]any{
			"choices": []map[string]any{{
				"message": map[string]string{"content": "ok"},
			}},
		})
	}))
	defer srv.Close()

	provider := RegisteredProvider{
		ID: "opencode",
		Config: FileConfig{
			APIKey:     "secret",
			AuthHeader: true,
		},
	}
	model := ResolvedModel{
		ID:      "opencode-v1",
		API:     APIOpenAICompletions,
		BaseURL: srv.URL,
	}

	p, err := NewProvider(provider, model)
	require.NoError(t, err)

	got, err := p.Complete(context.Background(), TurnRequest{
		UserPrompt: "hi",
		Model:      "opencode-v1",
	})
	require.NoError(t, err)
	require.Equal(t, "ok", got)
}
