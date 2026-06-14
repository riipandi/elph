package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestOpenAICompatibleCompleteToolCalls(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		tools, ok := body["tools"].([]any)
		require.True(t, ok)
		require.Len(t, tools, 1)

		_ = json.NewEncoder(w).Encode(map[string]any{
			"choices": []map[string]any{{
				"finish_reason": "tool_calls",
				"message": map[string]any{
					"tool_calls": []map[string]any{{
						"id":   "call_1",
						"type": "function",
						"function": map[string]string{
							"name":      "Read",
							"arguments": `{"path":"/tmp/a"}`,
						},
					}},
				},
			}},
		})
	}))
	defer srv.Close()

	p := NewOpenAICompatible(OpenAIOptions{
		APIKey:     "test-key",
		BaseURL:    srv.URL,
		AuthHeader: true,
	})

	got, err := p.Complete(context.Background(), TurnRequest{
		UserPrompt: "hi",
		Tools: []ToolDefinition{{
			Name:        "Read",
			Description: "Read a file",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"path": map[string]any{"type": "string"},
				},
				"required": []string{"path"},
			},
		}},
	})
	require.NoError(t, err)
	require.Equal(t, StopReasonToolUse, got.StopReason)
	require.Len(t, got.ToolCalls, 1)
	require.Equal(t, "Read", got.ToolCalls[0].Name)
}
