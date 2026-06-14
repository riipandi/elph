package providertests

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
	elphcompat "github.com/riipandi/elph/pkg/ai/providers/openaicompat"
	elphor "github.com/riipandi/elph/pkg/ai/providers/openrouter"
	"github.com/stretchr/testify/require"
)

func TestOpenAICompatComplete(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/chat/completions", r.URL.Path)
		require.Equal(t, "Bearer test-key", r.Header.Get("Authorization"))
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{
				"message": map[string]string{"content": "hello from gpt"},
			}},
		})
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{
		APIKey:       "test-key",
		BaseURL:      srv.URL,
		DefaultModel: "gpt-test",
		AuthHeader:   true,
	})
	testSimpleComplete(t, func(*testing.T) provider.Provider { return p }, "hello from gpt")
}

func TestOpenAICompatReasoningEffort(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		require.Equal(t, "medium", body["reasoning_effort"])
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{"message": map[string]string{"content": "done"}}},
		})
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking: provider.ThinkingConfig{
			Enabled:         true,
			ReasoningEffort: "medium",
		},
	})
	require.NoError(t, err)
	require.Equal(t, "done", got.Content)
}

func TestOpenRouterReasoning(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		reasoning, ok := body["reasoning"].(map[string]any)
		require.True(t, ok)
		require.Equal(t, "high", reasoning["effort"])
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{"message": map[string]string{"content": "done"}}},
		})
	}))
	defer srv.Close()

	p := elphor.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking: provider.ThinkingConfig{
			Enabled:         true,
			ReasoningEffort: "high",
			ThinkingFormat:  provider.ThinkingFormatOpenRouter,
		},
	})
	require.NoError(t, err)
	require.Equal(t, "done", got.Content)
}

func TestOpenAICompatEnableThinking(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		require.NoError(t, json.NewDecoder(r.Body).Decode(&body))
		require.Equal(t, true, body["enable_thinking"])
		_, hasEffort := body["reasoning_effort"]
		require.False(t, hasEffort)
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{"message": map[string]string{"content": "done"}}},
		})
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking: provider.ThinkingConfig{
			Enabled:        true,
			EnableThinking: true,
			ThinkingFormat: provider.ThinkingFormatQwen,
		},
		Compat: provider.Compat{
			ThinkingFormat:          string(provider.ThinkingFormatQwen),
			SupportsReasoningEffort: func() *bool { v := false; return &v }(),
		},
	})
	require.NoError(t, err)
	require.Equal(t, "done", got.Content)
}

func TestOpenAICompatStreamOpenCodeSSEComments(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeEventStreamResponse(w)
		_, _ = fmt.Fprint(w, ": OPENROUTER PROCESSING\n\n")
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"reasoning":"think "}}]}`)
		_, _ = fmt.Fprint(w, ": OPENROUTER PROCESSING\n\n")
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"content":"answer"}}]}`)
		_, _ = fmt.Fprintf(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	falseVal := false
	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	var thinking, content strings.Builder
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Thinking: provider.ThinkingConfig{
			Enabled:        true,
			EnableThinking: true,
			ThinkingFormat: provider.ThinkingFormatQwen,
		},
		Compat: provider.Compat{
			ThinkingFormat:          string(provider.ThinkingFormatQwen),
			SupportsReasoningEffort: &falseVal,
		},
		Stream: &provider.TurnStream{
			OnThinking: func(chunk string) { thinking.WriteString(chunk) },
			OnContent:  func(chunk string) { content.WriteString(chunk) },
		},
	})
	require.NoError(t, err)
	require.Equal(t, "think ", thinking.String())
	require.Equal(t, "answer", content.String())
	require.Equal(t, "think", got.Thinking)
	require.Equal(t, "answer", got.Content)
}

func TestOpenAICompatStreamOpenCodeReasoningField(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeEventStreamResponse(w)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"reasoning":"think "}}]}`)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"content":"answer"}}]}`)
		_, _ = fmt.Fprintf(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	var thinking, content strings.Builder
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Stream: &provider.TurnStream{
			OnThinking: func(chunk string) { thinking.WriteString(chunk) },
			OnContent:  func(chunk string) { content.WriteString(chunk) },
		},
	})
	require.NoError(t, err)
	require.Equal(t, "think ", thinking.String())
	require.Equal(t, "answer", content.String())
	require.Equal(t, "think", got.Thinking)
	require.Equal(t, "answer", got.Content)
}

func TestOpenAICompatStreamThinking(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeEventStreamResponse(w)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"reasoning_content":"think "}}]}`)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"content":"answer"}}]}`)
		_, _ = fmt.Fprintf(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	var thinking, content strings.Builder
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Stream: &provider.TurnStream{
			OnThinking: func(chunk string) { thinking.WriteString(chunk) },
			OnContent:  func(chunk string) { content.WriteString(chunk) },
		},
	})
	require.NoError(t, err)
	require.Equal(t, "think ", thinking.String())
	require.Equal(t, "answer", content.String())
	require.Equal(t, "think", got.Thinking)
	require.Equal(t, "answer", got.Content)
}

func TestOpenAICompatStreamMalformedEventFallback(t *testing.T) {
	calls := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if calls == 1 {
			writeEventStreamResponse(w)
			_, _ = fmt.Fprintf(w, "data:\n\n")
			return
		}
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{
				"message": map[string]string{"content": "recovered"},
			}},
		})
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Stream:     &provider.TurnStream{},
	})
	require.NoError(t, err)
	require.Equal(t, "recovered", got.Content)
	require.Equal(t, 2, calls)
}

func TestOpenAICompatStreamToolCalls(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeEventStreamResponse(w)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"AskUser","arguments":""}}]}}]}`)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"question\":\"Pick\"}"}}]}}]}`)
		_, _ = fmt.Fprintf(w, "data: %s\n\n", `{"choices":[{"finish_reason":"tool_calls"}]}`)
		_, _ = fmt.Fprintf(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "ask me",
		Stream:     &provider.TurnStream{},
		Tools: []provider.ToolDefinition{{
			Name: "AskUser", Description: "Ask the user",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"question": map[string]any{"type": "string"},
				},
				"required": []string{"question"},
			},
		}},
	})
	require.NoError(t, err)
	require.Equal(t, provider.StopReasonToolUse, got.StopReason)
	require.Len(t, got.ToolCalls, 1)
	require.Equal(t, "AskUser", got.ToolCalls[0].Name)
	require.JSONEq(t, `{"question":"Pick"}`, string(got.ToolCalls[0].Arguments))
}

func TestOpenAICompatToolCalls(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeJSONResponse(w, map[string]any{
			"choices": []map[string]any{{
				"finish_reason": "tool_calls",
				"message": map[string]any{
					"tool_calls": []map[string]any{{
						"id": "call_1", "type": "function",
						"function": map[string]string{"name": "Read", "arguments": `{"path":"/tmp/a"}`},
					}},
				},
			}},
		})
	}))
	defer srv.Close()

	p := elphcompat.New(elphopenai.Options{APIKey: "test-key", BaseURL: srv.URL, AuthHeader: true})
	got, err := p.Complete(context.Background(), provider.TurnRequest{
		UserPrompt: "hi",
		Tools: []provider.ToolDefinition{{
			Name: "Read", Description: "Read a file",
			Parameters: map[string]any{
				"type":       "object",
				"properties": map[string]any{"path": map[string]any{"type": "string"}},
				"required":   []string{"path"},
			},
		}},
	})
	require.NoError(t, err)
	require.Equal(t, provider.StopReasonToolUse, got.StopReason)
	require.Len(t, got.ToolCalls, 1)
	require.Equal(t, "Read", got.ToolCalls[0].Name)
}
