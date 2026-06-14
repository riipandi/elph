package providertests

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	elphant "github.com/riipandi/elph/pkg/ai/providers/anthropic"
	"github.com/stretchr/testify/require"
)

func TestAnthropicComplete(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/v1/messages", r.URL.Path)
		require.Equal(t, "test-key", r.Header.Get("x-api-key"))

		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		system, ok := body["system"].([]any)
		require.True(t, ok)
		block, ok := system[0].(map[string]any)
		require.True(t, ok)
		require.Equal(t, "sys", block["text"])

		writeJSONResponse(w, map[string]any{
			"content": []map[string]string{{"type": "text", "text": "hello from claude"}},
		})
	}))
	defer srv.Close()

	p := elphant.New(elphant.Options{
		ID: "anthropic", APIKey: "test-key", Model: "claude-test",
		BaseURL: srv.URL + "/v1", MaxTokens: 1024, Temperature: 0.4, TopP: 1.0,
	})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		SystemPrompt: "sys", UserPrompt: "hi", Model: "claude-test",
	})
	require.NoError(t, err)
	require.Equal(t, "hello from claude", got.Content)
}

func TestAnthropicThinkingBudget(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		thinking, ok := body["thinking"].(map[string]any)
		require.True(t, ok)
		require.Equal(t, "enabled", thinking["type"])
		require.Equal(t, float64(4096), thinking["budget_tokens"])
		writeJSONResponse(w, map[string]any{
			"content": []map[string]string{{"type": "text", "text": "done"}},
		})
	}))
	defer srv.Close()

	p := elphant.New(elphant.Options{APIKey: "test-key", BaseURL: srv.URL + "/v1"})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking:   provider.ThinkingConfig{Enabled: true, BudgetTokens: 4096},
	})
	require.NoError(t, err)
	require.Equal(t, "done", got.Content)
}

func TestAnthropicAdaptiveThinking(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		thinking, ok := body["thinking"].(map[string]any)
		require.True(t, ok)
		require.Equal(t, "adaptive", thinking["type"])
		output, ok := body["output_config"].(map[string]any)
		require.True(t, ok)
		require.Equal(t, "high", output["effort"])
		writeJSONResponse(w, map[string]any{
			"content": []map[string]string{{"type": "text", "text": "done"}},
		})
	}))
	defer srv.Close()

	p := elphant.New(elphant.Options{APIKey: "test-key", BaseURL: srv.URL + "/v1"})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking: provider.ThinkingConfig{
			Enabled: true, Adaptive: true, AdaptiveEffort: "high",
		},
	})
	require.NoError(t, err)
	require.Equal(t, "done", got.Content)
}
