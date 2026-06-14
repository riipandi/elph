package openai

import (
	"context"
	"fmt"
	"strings"

	openaisdk "github.com/openai/openai-go/v3"
	"github.com/openai/openai-go/v3/shared"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

type languageModel struct {
	opts   Options
	client openaisdk.Client
	hooks  Hooks
}

func (m *languageModel) ID() string {
	if m.opts.ID != "" {
		return m.opts.ID
	}
	return Name
}

func (m *languageModel) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	if m.opts.APIKey == "" && !m.opts.AuthHeader {
		return provider.TurnResult{}, provider.ErrMissingAPIKey
	}
	if req.Stream != nil {
		return m.completeStream(ctx, req)
	}
	return m.completeOnce(ctx, req)
}

func (m *languageModel) completeOnce(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	params := m.buildParams(req, false)
	resp, err := m.client.Chat.Completions.New(ctx, params)
	if err != nil {
		return provider.TurnResult{}, toProviderErr(err)
	}
	if len(resp.Choices) == 0 {
		return provider.TurnResult{}, fmt.Errorf("%s: empty response", m.ID())
	}

	result := turnResultFromChatChoice(resp.Choices[0], m.hooks)
	result.Usage = turnUsageFromCompletion(resp.Usage)
	if !resultValid(result) {
		return provider.TurnResult{}, fmt.Errorf("%s: empty response", m.ID())
	}
	return result, nil
}

func (m *languageModel) completeStream(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	params := m.buildParams(req, true)
	stream := m.client.Chat.Completions.NewStreaming(ctx, params)

	streamReasoning := m.hooks.StreamReasoning
	if streamReasoning == nil {
		streamReasoning = streamReasoningText
	}

	var thinking, content strings.Builder
	var usage provider.TurnUsage
	toolAcc := newStreamToolAccumulator()
	var finishReason string

	for stream.Next() {
		chunk := stream.Current()
		if chunk.Usage.JSON.TotalTokens.Valid() {
			usage.InputTokens = int(chunk.Usage.PromptTokens)
			usage.OutputTokens = int(chunk.Usage.CompletionTokens)
		}
		if len(chunk.Choices) == 0 {
			continue
		}
		choice := chunk.Choices[0]
		if choice.FinishReason != "" {
			finishReason = choice.FinishReason
		}
		delta := choice.Delta
		if rc := streamReasoning(delta); rc != "" {
			thinking.WriteString(rc)
			req.Stream.EmitThinking(rc)
		}
		if delta.Content != "" {
			content.WriteString(delta.Content)
			req.Stream.EmitContent(delta.Content)
		}
		if len(delta.ToolCalls) > 0 {
			toolAcc.absorbSDK(delta.ToolCalls)
		}
	}
	if err := stream.Err(); err != nil {
		return provider.TurnResult{}, toProviderErr(err)
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

func (m *languageModel) buildParams(req provider.TurnRequest, stream bool) openaisdk.ChatCompletionNewParams {
	model := req.Model
	if model == "" {
		model = m.opts.DefaultModel
	}

	chatMessagesFn := m.hooks.ChatMessages
	if chatMessagesFn == nil {
		chatMessagesFn = chatMessages
	}

	params := openaisdk.ChatCompletionNewParams{
		Model:    shared.ChatModel(model),
		Messages: chatMessagesFn(req.SystemPrompt, provider.BuildMessages(req), req.Thinking, req.Compat),
	}
	if m.opts.Temperature != 0 {
		params.Temperature = openaisdk.Float(m.opts.Temperature)
	}
	if m.opts.TopP != 0 {
		params.TopP = openaisdk.Float(m.opts.TopP)
	}

	maxField := "max_tokens"
	if req.Compat.MaxTokensField != "" {
		maxField = req.Compat.MaxTokensField
	}
	if m.opts.MaxTokens > 0 {
		switch maxField {
		case "max_completion_tokens":
			params.MaxCompletionTokens = openaisdk.Int(int64(m.opts.MaxTokens))
		default:
			params.MaxTokens = openaisdk.Int(int64(m.opts.MaxTokens))
		}
	}

	if m.hooks.PrepareParams != nil {
		m.hooks.PrepareParams(req, &params)
	}

	if tools := chatTools(req.Tools); len(tools) > 0 {
		params.Tools = tools
		params.ToolChoice = openaisdk.ChatCompletionToolChoiceOptionUnionParam{
			OfAuto: openaisdk.String(string(openaisdk.ChatCompletionToolChoiceOptionAutoAuto)),
		}
	}
	if stream && req.Compat.UsageInStreamingSupported() {
		params.StreamOptions = openaisdk.ChatCompletionStreamOptionsParam{
			IncludeUsage: openaisdk.Bool(true),
		}
	}
	return params
}
