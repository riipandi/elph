package provider

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"

	"github.com/riipandi/elph/pkg/ai/utils"
)

const anthropicVersion = "2023-06-01"

// AnthropicOptions configures an Anthropic Messages API provider.
type AnthropicOptions struct {
	ID          string
	APIKey      string
	Model       string
	BaseURL     string
	Headers     map[string]string
	MaxTokens   int
	Temperature float64
	TopP        float64
}

// Anthropic calls the Anthropic Messages API.
type Anthropic struct {
	IDName      string
	APIKey      string
	Model       string
	BaseURL     string
	Headers     map[string]string
	MaxTokens   int
	Temperature float64
	TopP        float64
	client      *http.Client
}

// NewAnthropic builds an Anthropic provider from explicit settings.
func NewAnthropic(opts AnthropicOptions) *Anthropic {
	maxTokens := opts.MaxTokens
	if maxTokens == 0 {
		maxTokens = defaultMaxTokens
	}
	return &Anthropic{
		IDName:      opts.ID,
		APIKey:      opts.APIKey,
		Model:       opts.Model,
		BaseURL:     strings.TrimRight(opts.BaseURL, "/"),
		Headers:     opts.Headers,
		MaxTokens:   maxTokens,
		Temperature: opts.Temperature,
		TopP:        opts.TopP,
		client:      utils.NewHTTPClient(),
	}
}

func (p *Anthropic) apiURL() string {
	if p.BaseURL == "" {
		return ""
	}
	return p.BaseURL + "/messages"
}

func (p *Anthropic) ID() string {
	if p.IDName == "" {
		return "anthropic"
	}
	return p.IDName
}

func (p *Anthropic) Complete(ctx context.Context, req TurnRequest) (TurnResult, error) {
	if p.APIKey == "" {
		return TurnResult{}, ErrMissingAPIKey
	}
	if req.Stream != nil {
		return p.completeStream(ctx, req)
	}
	return p.completeOnce(ctx, req)
}

func (p *Anthropic) completeOnce(ctx context.Context, req TurnRequest) (TurnResult, error) {
	model := req.Model
	if model == "" {
		model = p.Model
	}

	var out anthropicResponse
	body := p.buildRequestBody(req, model, false)
	err := utils.PostJSON(ctx, p.client, p.apiURL(), p.requestHeaders(), body, &out)
	if err != nil {
		return TurnResult{}, err
	}

	result := parseAnthropicResponse(out)
	if !anthropicResultValid(result) {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

func (p *Anthropic) completeStream(ctx context.Context, req TurnRequest) (TurnResult, error) {
	model := req.Model
	if model == "" {
		model = p.Model
	}

	body := p.buildRequestBody(req, model, true)

	var thinking, content strings.Builder
	var usage TurnUsage
	var toolCalls []ToolCall
	var currentTool *ToolCall
	var toolInput strings.Builder
	err := p.postAnthropicSSE(ctx, body, func(eventType string, data []byte) error {
		switch eventType {
		case "message_start":
			var evt struct {
				Message struct {
					Usage struct {
						InputTokens int `json:"input_tokens"`
					} `json:"usage"`
				} `json:"message"`
			}
			if err := json.Unmarshal(data, &evt); err == nil {
				usage.InputTokens = evt.Message.Usage.InputTokens
			}
		case "message_delta":
			var evt struct {
				Usage struct {
					OutputTokens int `json:"output_tokens"`
				} `json:"usage"`
			}
			if err := json.Unmarshal(data, &evt); err == nil && evt.Usage.OutputTokens > 0 {
				usage.OutputTokens = evt.Usage.OutputTokens
			}
		case "content_block_start":
			var evt struct {
				ContentBlock anthropicToolUseBlock `json:"content_block"`
			}
			if err := json.Unmarshal(data, &evt); err == nil && evt.ContentBlock.Type == "tool_use" {
				currentTool = &ToolCall{
					ID:   evt.ContentBlock.ID,
					Name: evt.ContentBlock.Name,
				}
				toolInput.Reset()
				if evt.ContentBlock.Input != nil {
					raw, _ := json.Marshal(evt.ContentBlock.Input)
					toolInput.Write(raw)
				}
			}
		case "content_block_delta":
			var evt anthropicDeltaEvent
			if err := json.Unmarshal(data, &evt); err != nil {
				return nil
			}
			switch evt.Delta.Type {
			case "thinking_delta":
				if evt.Delta.Thinking != "" {
					thinking.WriteString(evt.Delta.Thinking)
					req.Stream.emitThinking(evt.Delta.Thinking)
				}
			case "text_delta":
				if evt.Delta.Text != "" {
					content.WriteString(evt.Delta.Text)
					req.Stream.emitContent(evt.Delta.Text)
				}
			case "input_json_delta":
				if evt.Delta.PartialJSON != "" {
					toolInput.WriteString(evt.Delta.PartialJSON)
				}
			}
		case "content_block_stop":
			if currentTool != nil {
				args := json.RawMessage(toolInput.String())
				if len(args) == 0 {
					args = json.RawMessage("{}")
				}
				currentTool.Arguments = args
				toolCalls = append(toolCalls, *currentTool)
				currentTool = nil
			}
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
		ToolCalls: toolCalls,
	}
	if len(result.ToolCalls) > 0 {
		result.StopReason = StopReasonToolUse
	} else {
		result.StopReason = StopReasonEndTurn
	}
	if !anthropicResultValid(result) {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

type anthropicDelta struct {
	Type        string `json:"type"`
	Text        string `json:"text"`
	Thinking    string `json:"thinking"`
	PartialJSON string `json:"partial_json"`
}

type anthropicDeltaEvent struct {
	Delta anthropicDelta `json:"delta"`
}

func (p *Anthropic) postAnthropicSSE(ctx context.Context, body map[string]any, onEvent func(eventType string, data []byte) error) error {
	payload, err := json.Marshal(body)
	if err != nil {
		return fmt.Errorf("encode request: %w", err)
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, p.apiURL(), bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("build request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "text/event-stream")
	for key, value := range p.requestHeaders() {
		req.Header.Set(key, value)
	}

	resp, err := p.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		raw, _ := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
		return fmt.Errorf("upstream %s: %s", resp.Status, string(bytes.TrimSpace(raw)))
	}

	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	var eventType string
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, "event:") {
			eventType = strings.TrimSpace(strings.TrimPrefix(line, "event:"))
			continue
		}
		if !strings.HasPrefix(line, "data:") {
			continue
		}
		data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
		if data == "" {
			continue
		}
		if err := onEvent(eventType, []byte(data)); err != nil {
			return err
		}
	}
	if err := scanner.Err(); err != nil {
		return fmt.Errorf("read stream: %w", err)
	}
	return nil
}

func (p *Anthropic) buildRequestBody(req TurnRequest, model string, stream bool) map[string]any {
	body := map[string]any{
		"model":       model,
		"max_tokens":  p.MaxTokens,
		"temperature": p.Temperature,
		"top_p":       p.TopP,
		"messages":    AnthropicMessages(BuildMessages(req)),
	}
	if stream {
		body["stream"] = true
	}
	if strings.TrimSpace(req.SystemPrompt) != "" {
		body["system"] = req.SystemPrompt
	}
	if tools := AnthropicTools(req.Tools); len(tools) > 0 {
		body["tools"] = tools
	}
	applyAnthropicThinking(body, req.Thinking)
	return body
}

func applyAnthropicThinking(body map[string]any, thinking ThinkingConfig) {
	if !thinking.Enabled {
		return
	}
	if thinking.Adaptive {
		body["thinking"] = map[string]any{"type": "adaptive"}
		if thinking.AdaptiveEffort != "" {
			body["output_config"] = map[string]any{"effort": thinking.AdaptiveEffort}
		}
		return
	}
	if thinking.BudgetTokens > 0 {
		body["thinking"] = map[string]any{
			"type":          "enabled",
			"budget_tokens": thinking.BudgetTokens,
		}
	}
}

func (p *Anthropic) requestHeaders() map[string]string {
	headers := make(map[string]string, len(p.Headers)+2)
	for key, value := range p.Headers {
		headers[key] = value
	}
	if headers["x-api-key"] == "" {
		headers["x-api-key"] = p.APIKey
	}
	if headers["anthropic-version"] == "" {
		headers["anthropic-version"] = anthropicVersion
	}
	return headers
}
