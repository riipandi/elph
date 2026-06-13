package provider

import (
	"context"
	"fmt"
	"net/http"
	"strings"

	"github.com/riipandi/elph/pkg/ai/utils"
)

const anthropicVersion = "2023-06-01"

// AnthropicOptions configures an Anthropic Messages API provider.
type AnthropicOptions struct {
	ID        string
	APIKey    string
	Model     string
	BaseURL   string
	Headers   map[string]string
	MaxTokens   int
	Temperature float64
}

// Anthropic calls the Anthropic Messages API.
type Anthropic struct {
	IDName    string
	APIKey    string
	Model     string
	BaseURL   string
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

func (p *Anthropic) Complete(ctx context.Context, req TurnRequest) (string, error) {
	if p.APIKey == "" {
		return "", ErrMissingAPIKey
	}

	model := req.Model
	if model == "" {
		model = p.Model
	}

	type message struct {
		Role    string `json:"role"`
		Content string `json:"content"`
	}
	type request struct {
		Model     string    `json:"model"`
		MaxTokens   int       `json:"max_tokens"`
		Temperature float64   `json:"temperature"`
		System      string    `json:"system,omitempty"`
		Messages  []message `json:"messages"`
	}
	type contentBlock struct {
		Type string `json:"type"`
		Text string `json:"text"`
	}
	type response struct {
		Content []contentBlock `json:"content"`
	}

	var out response
	err := utils.PostJSON(ctx, p.client, p.apiURL(), p.requestHeaders(), request{
		Model:     model,
		MaxTokens:   p.MaxTokens,
		Temperature: p.Temperature,
		System:      req.SystemPrompt,
		Messages:  []message{{Role: "user", Content: req.UserPrompt}},
	}, &out)
	if err != nil {
		return "", err
	}

	var text string
	for _, block := range out.Content {
		if block.Type == "text" && block.Text != "" {
			if text != "" {
				text += "\n"
			}
			text += block.Text
		}
	}
	if text == "" {
		return "", fmt.Errorf("%s: empty response", p.ID())
	}
	return text, nil
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
