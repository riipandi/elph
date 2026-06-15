package openai

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	openaisdk "github.com/openai/openai-go/v3"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/providers/internal/httpheaders"
	"github.com/riipandi/elph/pkg/ai/utils"
)

func preferRawSSE(req provider.TurnRequest) bool {
	if req.Compat.ThinkingFormat == string(provider.ThinkingFormatQwen) {
		return true
	}
	if !req.Thinking.Enabled {
		return false
	}
	switch req.Thinking.ThinkingFormat {
	case provider.ThinkingFormatQwen:
		return true
	default:
		return req.Thinking.EnableThinking
	}
}

func (m *languageModel) completeStreamOpenAICompat(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	if preferRawSSE(req) {
		result, err := m.completeStreamSSE(ctx, req)
		if err == nil {
			return result, nil
		}
		// Only retry with the SDK stream on malformed SSE JSON. Propagate rate
		// limits, stalls, and transport errors immediately instead of doubling
		// wait time with a second hung request.
		if !provider.IsStreamJSONError(err) {
			return provider.TurnResult{}, err
		}
	}
	return m.completeStream(ctx, req)
}

type sseStreamChunk struct {
	Usage *struct {
		PromptTokens     int `json:"prompt_tokens"`
		CompletionTokens int `json:"completion_tokens"`
	} `json:"usage"`
	Choices []struct {
		FinishReason string          `json:"finish_reason"`
		Delta        json.RawMessage `json:"delta"`
	} `json:"choices"`
}

type sseStreamDelta struct {
	Content   string `json:"content"`
	ToolCalls []struct {
		Index    int    `json:"index"`
		ID       string `json:"id"`
		Function struct {
			Name      string `json:"name"`
			Arguments string `json:"arguments"`
		} `json:"function"`
	} `json:"tool_calls"`
}

func (m *languageModel) completeStreamSSE(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	params := m.buildParams(req, true)
	body, err := chatCompletionStreamBody(params)
	if err != nil {
		return provider.TurnResult{}, err
	}

	headers := httpheaders.ResolveHeaders(m.opts.Headers, m.opts.UserAgent, httpheaders.DefaultUserAgent(""))
	if m.opts.APIKey != "" {
		headers["Authorization"] = "Bearer " + m.opts.APIKey
	} else if m.opts.AuthHeader {
		headers["Authorization"] = "Bearer "
	}

	var thinking, content strings.Builder
	var usage provider.TurnUsage
	toolAcc := newStreamToolAccumulator()
	var finishReason string

	url := strings.TrimRight(m.opts.BaseURL, "/") + "/chat/completions"
	err = utils.PostSSE(ctx, utils.NewStreamingHTTPClient(), url, headers, body, req.StreamStallTimeout, func(data []byte) error {
		if streamErr, ok := provider.ProviderErrorFromStreamData(data); ok {
			return streamErr
		}
		var chunk sseStreamChunk
		if err := json.Unmarshal(data, &chunk); err != nil {
			return fmt.Errorf("decode stream chunk: %w", err)
		}
		if chunk.Usage != nil {
			usage.InputTokens = chunk.Usage.PromptTokens
			usage.OutputTokens = chunk.Usage.CompletionTokens
		}
		if len(chunk.Choices) == 0 {
			return nil
		}
		choice := chunk.Choices[0]
		if choice.FinishReason != "" {
			finishReason = choice.FinishReason
		}
		if len(choice.Delta) == 0 {
			return nil
		}

		if rc := reasoningText(nil, string(choice.Delta)); rc != "" {
			thinking.WriteString(rc)
			if req.Stream != nil {
				req.Stream.EmitThinking(rc)
			}
		}

		var delta sseStreamDelta
		if err := json.Unmarshal(choice.Delta, &delta); err != nil {
			return fmt.Errorf("decode stream delta: %w", err)
		}
		if delta.Content != "" {
			content.WriteString(delta.Content)
			if req.Stream != nil {
				req.Stream.EmitContent(delta.Content)
			}
		}
		for _, call := range delta.ToolCalls {
			toolAcc.absorbJSON(call.Index, call.ID, call.Function.Name, call.Function.Arguments)
		}
		return nil
	})
	if err != nil {
		return provider.TurnResult{}, provider.WrapStreamError(err)
	}

	result := provider.TurnResult{
		Thinking:  strings.TrimSpace(thinking.String()),
		Content:   strings.TrimSpace(content.String()),
		Usage:     usage,
		ToolCalls: toolAcc.result(),
	}
	if finishReason == "tool_calls" || len(result.ToolCalls) > 0 {
		result.StopReason = provider.StopReasonToolUse
	} else {
		result.StopReason = provider.StopReasonEndTurn
	}
	if !resultValid(result) {
		return provider.TurnResult{}, fmt.Errorf("%s: empty response", m.ID())
	}
	return result, nil
}

func chatCompletionStreamBody(params openaisdk.ChatCompletionNewParams) (map[string]any, error) {
	raw, err := json.Marshal(params)
	if err != nil {
		return nil, fmt.Errorf("marshal stream params: %w", err)
	}
	var body map[string]any
	if err := json.Unmarshal(raw, &body); err != nil {
		return nil, fmt.Errorf("decode stream params: %w", err)
	}
	body["stream"] = true
	return body, nil
}
