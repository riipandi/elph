package provider

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"github.com/riipandi/elph/pkg/ai/utils"
)

const defaultOpenAIBaseURL = "https://api.openai.com/v1"

// OpenAIOptions configures an OpenAI-compatible chat completions provider.
type OpenAIOptions struct {
	ID           string
	APIKey       string
	BaseURL      string
	DefaultModel string
	Headers      map[string]string
	AuthHeader   bool
	MaxTokens    int
	Temperature  float64
	TopP         float64
}

// OpenAICompatible calls an OpenAI-style chat completions endpoint.
type OpenAICompatible struct {
	IDName       string
	APIKey       string
	BaseURL      string
	DefaultModel string
	Headers      map[string]string
	AuthHeader   bool
	MaxTokens    int
	Temperature  float64
	TopP         float64
	client       *http.Client
}

// NewOpenAICompatible builds a provider for a compatible HTTP API.
func NewOpenAICompatible(opts OpenAIOptions) *OpenAICompatible {
	baseURL := opts.BaseURL
	if baseURL == "" {
		baseURL = defaultOpenAIBaseURL
	}
	baseURL = strings.TrimRight(baseURL, "/")

	maxTokens := opts.MaxTokens
	if maxTokens == 0 {
		maxTokens = defaultMaxTokens
	}
	return &OpenAICompatible{
		IDName:       opts.ID,
		APIKey:       opts.APIKey,
		BaseURL:      baseURL,
		DefaultModel: opts.DefaultModel,
		Headers:      opts.Headers,
		AuthHeader:   opts.AuthHeader,
		MaxTokens:    maxTokens,
		Temperature:  opts.Temperature,
		TopP:         opts.TopP,
		client:       utils.NewHTTPClient(),
	}
}

func (p *OpenAICompatible) ID() string {
	if p.IDName == "" {
		return "openai"
	}
	return p.IDName
}

func (p *OpenAICompatible) Complete(ctx context.Context, req TurnRequest) (TurnResult, error) {
	if p.APIKey == "" && !p.AuthHeader {
		return TurnResult{}, ErrMissingAPIKey
	}
	if req.Stream != nil {
		return p.completeStream(ctx, req)
	}
	return p.completeOnce(ctx, req)
}

func (p *OpenAICompatible) completeOnce(ctx context.Context, req TurnRequest) (TurnResult, error) {
	type response struct {
		Choices []openAIChoice `json:"choices"`
	}

	body, err := p.buildRequest(req, false)
	if err != nil {
		return TurnResult{}, err
	}

	var out response
	url := p.BaseURL + "/chat/completions"
	if err := utils.PostJSON(ctx, p.client, url, p.requestHeaders(), body, &out); err != nil {
		return TurnResult{}, err
	}
	if len(out.Choices) == 0 {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}

	result := parseOpenAIChoice(out.Choices[0])
	if !openAIResultValid(result) {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

func (p *OpenAICompatible) completeStream(ctx context.Context, req TurnRequest) (TurnResult, error) {
	body, err := p.buildRequest(req, true)
	if err != nil {
		return TurnResult{}, err
	}

	var thinking, content strings.Builder
	var usage TurnUsage
	toolAcc := newOpenAIStreamToolAccumulator()
	var finishReason string
	err = utils.PostSSE(ctx, p.client, p.BaseURL+"/chat/completions", p.requestHeaders(), body, func(data []byte) error {
		var chunk openAIStreamChunk
		if err := json.Unmarshal(data, &chunk); err != nil {
			return nil
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
		delta := choice.Delta
		if delta.ReasoningContent != "" {
			thinking.WriteString(delta.ReasoningContent)
			req.Stream.emitThinking(delta.ReasoningContent)
		}
		if delta.Reasoning != "" {
			thinking.WriteString(delta.Reasoning)
			req.Stream.emitThinking(delta.Reasoning)
		}
		if delta.Content != "" {
			content.WriteString(delta.Content)
			req.Stream.emitContent(delta.Content)
		}
		if len(delta.ToolCalls) > 0 {
			toolAcc.absorb(delta.ToolCalls)
		}
		return nil
	})
	if err != nil {
		return TurnResult{}, err
	}

	result := TurnResult{
		Thinking:  strings.TrimSpace(thinking.String()),
		Content:   strings.TrimSpace(content.String()),
		Usage:     usage,
		ToolCalls: toolAcc.result(),
	}
	if finishReason == "tool_calls" || len(result.ToolCalls) > 0 {
		result.StopReason = StopReasonToolUse
	} else {
		result.StopReason = StopReasonEndTurn
	}
	if !openAIResultValid(result) {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

type openAIStreamUsage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
}

type openAIStreamDelta struct {
	Content          string                 `json:"content"`
	ReasoningContent string                 `json:"reasoning_content"`
	Reasoning        string                 `json:"reasoning"`
	ToolCalls        []openAIStreamToolCall `json:"tool_calls"`
}

type openAIStreamChoice struct {
	Delta        openAIStreamDelta `json:"delta"`
	FinishReason string            `json:"finish_reason"`
}

type openAIStreamChunk struct {
	Choices []openAIStreamChoice `json:"choices"`
	Usage   *openAIStreamUsage   `json:"usage,omitempty"`
}

func (p *OpenAICompatible) buildRequest(req TurnRequest, stream bool) (map[string]any, error) {
	model := req.Model
	if model == "" {
		model = p.DefaultModel
	}

	body := map[string]any{
		"model":       model,
		"messages":    OpenAIMessages(req.SystemPrompt, BuildMessages(req), req.Thinking, req.Compat),
		"temperature": p.Temperature,
		"top_p":       p.TopP,
	}
	maxField := "max_tokens"
	if req.Compat.MaxTokensField != "" {
		maxField = req.Compat.MaxTokensField
	}
	if p.MaxTokens > 0 {
		body[maxField] = p.MaxTokens
	}
	applyOpenAIThinking(body, req.Thinking, req.Compat)
	if tools := OpenAITools(req.Tools); len(tools) > 0 {
		body["tools"] = tools
		body["tool_choice"] = "auto"
	}
	if stream {
		body["stream"] = true
		if req.Compat.supportsUsageInStreaming() {
			body["stream_options"] = map[string]any{"include_usage": true}
		}
	}
	return body, nil
}

func applyOpenAIThinking(body map[string]any, thinking ThinkingConfig, compat Compat) {
	if !thinking.Enabled {
		return
	}
	switch thinking.ThinkingFormat {
	case ThinkingFormatOpenRouter:
		if thinking.ReasoningEffort != "" {
			body["reasoning"] = map[string]any{"effort": thinking.ReasoningEffort}
		}
	case ThinkingFormatQwen:
		body["enable_thinking"] = thinking.EnableThinking
	default:
		if compat.supportsReasoningEffort() && thinking.ReasoningEffort != "" {
			body["reasoning_effort"] = thinking.ReasoningEffort
		}
	}
}

func (p *OpenAICompatible) requestHeaders() map[string]string {
	headers := make(map[string]string, len(p.Headers)+1)
	for key, value := range p.Headers {
		headers[key] = value
	}
	if p.AuthHeader || (p.APIKey != "" && headers["Authorization"] == "") {
		headers["Authorization"] = "Bearer " + p.APIKey
	}
	return headers
}
