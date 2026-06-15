// Package anthropic provides an Anthropic Messages API adapter for Elph providers.
package anthropic

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/anthropics/anthropic-sdk-go"
	"github.com/anthropics/anthropic-sdk-go/option"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/providers/internal/httpheaders"
	"github.com/riipandi/elph/pkg/ai/utils"
)

const (
	// Name is the default provider identifier.
	Name = "anthropic"
	// DefaultURL is the default Anthropic API base URL.
	DefaultURL = "https://api.anthropic.com"
)

// Options configures an Anthropic provider.
type Options struct {
	ID          string
	APIKey      string
	Model       string
	BaseURL     string
	Headers     map[string]string
	MaxTokens   int
	Temperature float64
	TopP        float64
	UserAgent   string
}

type languageModel struct {
	opts   Options
	client anthropic.Client
}

// New builds a provider.Provider backed by anthropic-sdk-go.
func New(opts Options) provider.Provider {
	maxTokens := provider.MaxTokensOrDefault(opts.MaxTokens)

	clientOpts := []option.RequestOption{
		option.WithBaseURL(normalizeBaseURL(opts.BaseURL)),
		option.WithHTTPClient(utils.NewHTTPClient()),
	}
	if opts.APIKey != "" {
		clientOpts = append(clientOpts, option.WithAPIKey(opts.APIKey))
	}
	resolved := httpheaders.ResolveHeaders(opts.Headers, opts.UserAgent, defaultUserAgent())
	for key, value := range resolved {
		clientOpts = append(clientOpts, option.WithHeader(key, value))
	}

	return &languageModel{
		opts: Options{
			ID:          opts.ID,
			APIKey:      opts.APIKey,
			Model:       SanitizeModelID(opts.Model),
			BaseURL:     strings.TrimRight(opts.BaseURL, "/"),
			Headers:     opts.Headers,
			MaxTokens:   maxTokens,
			Temperature: opts.Temperature,
			TopP:        opts.TopP,
			UserAgent:   opts.UserAgent,
		},
		client: anthropic.NewClient(clientOpts...),
	}
}

func normalizeBaseURL(baseURL string) string {
	baseURL = strings.TrimRight(strings.TrimSpace(baseURL), "/")
	if baseURL == "" {
		return DefaultURL
	}
	if strings.HasSuffix(baseURL, "/v1") {
		baseURL = strings.TrimSuffix(baseURL, "/v1")
	}
	return baseURL
}

func (p *languageModel) ID() string {
	if p.opts.ID == "" {
		return Name
	}
	return p.opts.ID
}

func emitAnthropicTurnResultStream(stream *provider.TurnStream, result provider.TurnResult) {
	if stream == nil {
		return
	}
	if thinking := strings.TrimSpace(result.Thinking); thinking != "" {
		stream.EmitThinking(thinking)
	}
	if content := strings.TrimSpace(result.Content); content != "" {
		stream.EmitContent(content)
	}
}

func (p *languageModel) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	if p.opts.APIKey == "" {
		return provider.TurnResult{}, provider.ErrMissingAPIKey
	}
	if req.Stream != nil {
		result, err := p.completeStream(ctx, req)
		if err != nil && provider.ShouldStreamNonStreamingFallback(err) {
			fallback := req
			fallback.Stream = nil
			once, onceErr := p.completeOnce(ctx, fallback)
			if onceErr != nil {
				return once, onceErr
			}
			emitAnthropicTurnResultStream(req.Stream, once)
			return once, nil
		}
		return result, err
	}
	return p.completeOnce(ctx, req)
}

func (p *languageModel) completeOnce(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	params := p.buildParams(req)
	resp, err := p.client.Messages.New(ctx, params)
	if err != nil {
		return provider.TurnResult{}, toProviderErr(err)
	}

	result := turnResultFromMessage(resp)
	if !resultValid(result) {
		return provider.TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

func (p *languageModel) completeStream(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	params := p.buildParams(req)
	stream := p.client.Messages.NewStreaming(ctx, params)

	var thinking, content strings.Builder
	var usage provider.TurnUsage
	var toolCalls []provider.ToolCall
	var currentTool *provider.ToolCall
	var toolInput strings.Builder

	for stream.Next() {
		event := stream.Current()
		switch variant := event.AsAny().(type) {
		case anthropic.MessageStartEvent:
			usage.InputTokens = int(variant.Message.Usage.InputTokens)
		case anthropic.MessageDeltaEvent:
			if variant.Usage.OutputTokens > 0 {
				usage.OutputTokens = int(variant.Usage.OutputTokens)
			}
		case anthropic.ContentBlockStartEvent:
			if variant.ContentBlock.Type == "tool_use" {
				currentTool = &provider.ToolCall{
					ID:   variant.ContentBlock.ID,
					Name: variant.ContentBlock.Name,
				}
				toolInput.Reset()
			}
		case anthropic.ContentBlockDeltaEvent:
			delta := variant.Delta
			switch delta.Type {
			case "thinking_delta":
				if delta.Thinking != "" {
					thinking.WriteString(delta.Thinking)
					req.Stream.EmitThinking(delta.Thinking)
				}
			case "text_delta":
				if delta.Text != "" {
					content.WriteString(delta.Text)
					req.Stream.EmitContent(delta.Text)
				}
			case "input_json_delta":
				if delta.PartialJSON != "" {
					toolInput.WriteString(delta.PartialJSON)
				}
			}
		case anthropic.ContentBlockStopEvent:
			if currentTool != nil {
				currentTool.Arguments = provider.NormalizeToolArguments(json.RawMessage(toolInput.String()))
				toolCalls = append(toolCalls, *currentTool)
				currentTool = nil
			}
		}
	}
	if err := stream.Err(); err != nil {
		return provider.TurnResult{}, toProviderErr(err)
	}

	result := provider.TurnResult{
		Thinking:  strings.TrimSpace(thinking.String()),
		Content:   strings.TrimSpace(content.String()),
		Usage:     usage,
		ToolCalls: toolCalls,
	}
	if len(result.ToolCalls) > 0 {
		result.StopReason = provider.StopReasonToolUse
	} else {
		result.StopReason = provider.StopReasonEndTurn
	}
	if !resultValid(result) {
		return provider.TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

func (p *languageModel) buildParams(req provider.TurnRequest) anthropic.MessageNewParams {
	model := SanitizeModelID(req.Model)
	if model == "" {
		model = p.opts.Model
	}

	params := anthropic.MessageNewParams{
		Model:       anthropic.Model(model),
		MaxTokens:   int64(p.opts.MaxTokens),
		Messages:    anthropicMessages(provider.BuildMessages(req)),
		Temperature: anthropic.Float(p.opts.Temperature),
		TopP:        anthropic.Float(p.opts.TopP),
	}
	if strings.TrimSpace(req.SystemPrompt) != "" {
		params.System = []anthropic.TextBlockParam{{Text: req.SystemPrompt}}
	}
	if tools := anthropicTools(req.Tools); len(tools) > 0 {
		params.Tools = tools
	}
	applyThinkingParams(&params, req.Thinking)
	return params
}

func applyThinkingParams(params *anthropic.MessageNewParams, thinking provider.ThinkingConfig) {
	if !thinking.Enabled {
		return
	}
	if thinking.Adaptive {
		params.Thinking = anthropic.ThinkingConfigParamUnion{
			OfAdaptive: &anthropic.ThinkingConfigAdaptiveParam{},
		}
		if thinking.AdaptiveEffort != "" {
			params.OutputConfig = anthropic.OutputConfigParam{
				Effort: anthropic.OutputConfigEffort(thinking.AdaptiveEffort),
			}
		}
		return
	}
	if thinking.BudgetTokens > 0 {
		params.Thinking = anthropic.ThinkingConfigParamOfEnabled(int64(thinking.BudgetTokens))
	}
}
