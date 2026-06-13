package provider

import (
	"context"
	"fmt"
	"net/http"
	"strings"
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
	client       *http.Client
}

// NewOpenAICompatible builds a provider for a compatible HTTP API.
func NewOpenAICompatible(opts OpenAIOptions) *OpenAICompatible {
	baseURL := opts.BaseURL
	if baseURL == "" {
		baseURL = defaultOpenAIBaseURL
	}
	baseURL = strings.TrimRight(baseURL, "/")

	model := opts.DefaultModel
	maxTokens := opts.MaxTokens
	if maxTokens == 0 {
		maxTokens = defaultMaxTokens
	}

	return &OpenAICompatible{
		IDName:       opts.ID,
		APIKey:       opts.APIKey,
		BaseURL:      baseURL,
		DefaultModel: model,
		Headers:      opts.Headers,
		AuthHeader:   opts.AuthHeader,
		MaxTokens:    maxTokens,
		client:       newHTTPClient(),
	}
}

func (p *OpenAICompatible) ID() string {
	if p.IDName == "" {
		return "openai"
	}
	return p.IDName
}

func (p *OpenAICompatible) Complete(ctx context.Context, req TurnRequest) (string, error) {
	if p.APIKey == "" && !p.AuthHeader {
		return "", ErrMissingAPIKey
	}

	model := req.Model
	if model == "" {
		model = p.DefaultModel
	}

	type message struct {
		Role    string `json:"role"`
		Content string `json:"content"`
	}
	type request struct {
		Model     string    `json:"model"`
		Messages  []message `json:"messages"`
		MaxTokens int       `json:"max_tokens,omitempty"`
	}
	type choice struct {
		Message message `json:"message"`
	}
	type response struct {
		Choices []choice `json:"choices"`
	}

	messages := make([]message, 0, 2)
	if strings.TrimSpace(req.SystemPrompt) != "" {
		messages = append(messages, message{Role: "system", Content: req.SystemPrompt})
	}
	messages = append(messages, message{Role: "user", Content: req.UserPrompt})

	var out response
	url := p.BaseURL + "/chat/completions"
	err := postJSON(ctx, p.client, url, p.requestHeaders(), request{
		Model:     model,
		Messages:  messages,
		MaxTokens: p.MaxTokens,
	}, &out)
	if err != nil {
		return "", err
	}
	if len(out.Choices) == 0 || strings.TrimSpace(out.Choices[0].Message.Content) == "" {
		return "", fmt.Errorf("%s: empty response", p.ID())
	}
	return out.Choices[0].Message.Content, nil
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
