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
	type message struct {
		Role             string `json:"role"`
		Content          string `json:"content"`
		ReasoningContent string `json:"reasoning_content,omitempty"`
	}
	type request struct {
		Model       string    `json:"model"`
		Messages    []message `json:"messages"`
		MaxTokens   int       `json:"max_tokens,omitempty"`
		Temperature float64   `json:"temperature"`
	}
	type choice struct {
		Message message `json:"message"`
	}
	type response struct {
		Choices []choice `json:"choices"`
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

	result := TurnResult{
		Thinking: strings.TrimSpace(out.Choices[0].Message.ReasoningContent),
		Content:  strings.TrimSpace(out.Choices[0].Message.Content),
	}
	if result.Thinking == "" && result.Content == "" {
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
	err = utils.PostSSE(ctx, p.client, p.BaseURL+"/chat/completions", p.requestHeaders(), body, func(data []byte) error {
		var chunk openAIStreamChunk
		if err := json.Unmarshal(data, &chunk); err != nil {
			return nil
		}
		if len(chunk.Choices) == 0 {
			return nil
		}
		delta := chunk.Choices[0].Delta
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

type openAIStreamDelta struct {
	Content          string `json:"content"`
	ReasoningContent string `json:"reasoning_content"`
	Reasoning        string `json:"reasoning"`
}

type openAIStreamChoice struct {
	Delta openAIStreamDelta `json:"delta"`
}

type openAIStreamChunk struct {
	Choices []openAIStreamChoice `json:"choices"`
}

func (p *OpenAICompatible) buildRequest(req TurnRequest, stream bool) (map[string]any, error) {
	model := req.Model
	if model == "" {
		model = p.DefaultModel
	}

	messages := make([]map[string]string, 0, 2)
	if strings.TrimSpace(req.SystemPrompt) != "" {
		messages = append(messages, map[string]string{"role": "system", "content": req.SystemPrompt})
	}
	messages = append(messages, map[string]string{"role": "user", "content": req.UserPrompt})

	body := map[string]any{
		"model":       model,
		"messages":    messages,
		"temperature": p.Temperature,
	}
	if p.MaxTokens > 0 {
		body["max_tokens"] = p.MaxTokens
	}
	if stream {
		body["stream"] = true
	}
	return body, nil
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
