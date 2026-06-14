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

type anthropicContentBlock struct {
	Type     string `json:"type"`
	Text     string `json:"text"`
	Thinking string `json:"thinking"`
}

func (p *Anthropic) completeOnce(ctx context.Context, req TurnRequest) (TurnResult, error) {
	type message struct {
		Role    string `json:"role"`
		Content string `json:"content"`
	}
	type request struct {
		Model       string    `json:"model"`
		MaxTokens   int       `json:"max_tokens"`
		Temperature float64   `json:"temperature"`
		System      string    `json:"system,omitempty"`
		Messages    []message `json:"messages"`
	}
	type response struct {
		Content []anthropicContentBlock `json:"content"`
	}

	model := req.Model
	if model == "" {
		model = p.Model
	}

	var out response
	err := utils.PostJSON(ctx, p.client, p.apiURL(), p.requestHeaders(), request{
		Model:       model,
		MaxTokens:   p.MaxTokens,
		Temperature: p.Temperature,
		System:      req.SystemPrompt,
		Messages:    []message{{Role: "user", Content: req.UserPrompt}},
	}, &out)
	if err != nil {
		return TurnResult{}, err
	}

	result := parseAnthropicContent(out.Content)
	if result.Thinking == "" && result.Content == "" {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

func (p *Anthropic) completeStream(ctx context.Context, req TurnRequest) (TurnResult, error) {
	model := req.Model
	if model == "" {
		model = p.Model
	}

	body := map[string]any{
		"model":       model,
		"max_tokens":  p.MaxTokens,
		"temperature": p.Temperature,
		"stream":      true,
		"messages":    []map[string]string{{"role": "user", "content": req.UserPrompt}},
	}
	if strings.TrimSpace(req.SystemPrompt) != "" {
		body["system"] = req.SystemPrompt
	}

	var thinking, content strings.Builder
	err := p.postAnthropicSSE(ctx, body, func(eventType string, data []byte) error {
		switch eventType {
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
			}
		}
		return nil
	})
	if err != nil {
		return TurnResult{}, err
	}

	result := TurnResult{
		Thinking: strings.TrimSpace(thinking.String()),
		Content:  strings.TrimSpace(content.String()),
	}
	if result.Thinking == "" && result.Content == "" {
		return TurnResult{}, fmt.Errorf("%s: empty response", p.ID())
	}
	return result, nil
}

type anthropicDelta struct {
	Type     string `json:"type"`
	Text     string `json:"text"`
	Thinking string `json:"thinking"`
}

type anthropicDeltaEvent struct {
	Delta anthropicDelta `json:"delta"`
}

func parseAnthropicContent(blocks []anthropicContentBlock) TurnResult {
	var result TurnResult
	for _, block := range blocks {
		switch block.Type {
		case "thinking":
			if block.Thinking != "" {
				if result.Thinking != "" {
					result.Thinking += "\n"
				}
				result.Thinking += block.Thinking
			}
		case "text":
			if block.Text != "" {
				if result.Content != "" {
					result.Content += "\n"
				}
				result.Content += block.Text
			}
		}
	}
	return result
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
