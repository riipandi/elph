package provider

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"strings"
)

const (
	openAIAPIKeyEnv      = "OPENAI_API_KEY"
	openAIModelEnv       = "OPENAI_MODEL"
	openAIBaseURLEnv     = "OPENAI_BASE_URL"
	defaultOpenAIBaseURL = "https://api.openai.com/v1"
	defaultOpenAIModel   = "gpt-4o-mini"
)

// OpenAICompatible calls an OpenAI-style chat completions endpoint.
type OpenAICompatible struct {
	IDName       string
	APIKey       string
	BaseURL      string
	DefaultModel string
	client       *http.Client
}

// NewOpenAICompatible builds a provider for a compatible HTTP API.
func NewOpenAICompatible(id, apiKey, baseURL, model string) *OpenAICompatible {
	if baseURL == "" {
		baseURL = defaultOpenAIBaseURL
	}
	baseURL = strings.TrimRight(baseURL, "/")
	if model == "" {
		model = defaultOpenAIModel
	}
	return &OpenAICompatible{
		IDName:       id,
		APIKey:       apiKey,
		BaseURL:      baseURL,
		DefaultModel: model,
		client:       newHTTPClient(),
	}
}

// NewOpenAIFromEnv reads OPENAI_API_KEY and optional OPENAI_MODEL / OPENAI_BASE_URL.
func NewOpenAIFromEnv() (*OpenAICompatible, error) {
	apiKey := os.Getenv(openAIAPIKeyEnv)
	if apiKey == "" {
		return nil, ErrMissingAPIKey
	}
	return NewOpenAICompatible(
		IDOpenAI,
		apiKey,
		os.Getenv(openAIBaseURLEnv),
		os.Getenv(openAIModelEnv),
	), nil
}

func (p *OpenAICompatible) ID() string {
	if p.IDName == "" {
		return IDOpenAI
	}
	return p.IDName
}

func (p *OpenAICompatible) Complete(ctx context.Context, req TurnRequest) (string, error) {
	if p.APIKey == "" {
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
		Model    string    `json:"model"`
		Messages []message `json:"messages"`
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
	err := postJSON(ctx, p.client, url, map[string]string{
		"Authorization": "Bearer " + p.APIKey,
	}, request{
		Model:    model,
		Messages: messages,
	}, &out)
	if err != nil {
		return "", err
	}
	if len(out.Choices) == 0 || strings.TrimSpace(out.Choices[0].Message.Content) == "" {
		return "", fmt.Errorf("%s: empty response", p.ID())
	}
	return out.Choices[0].Message.Content, nil
}