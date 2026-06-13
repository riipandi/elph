package provider

import (
	"context"
	"fmt"
	"net/http"
	"os"
)

const (
	anthropicAPIURL        = "https://api.anthropic.com/v1/messages"
	anthropicVersion       = "2023-06-01"
	defaultAnthropicModel  = "claude-sonnet-4-20250514"
	anthropicAPIKeyEnv     = "ANTHROPIC_API_KEY"
	anthropicModelEnv      = "ANTHROPIC_MODEL"
)

// Anthropic calls the Anthropic Messages API.
type Anthropic struct {
	APIKey string
	Model  string
	APIURL string
	client *http.Client
}

// NewAnthropic builds an Anthropic provider from explicit settings.
func NewAnthropic(apiKey, model string) *Anthropic {
	if model == "" {
		model = defaultAnthropicModel
	}
	return &Anthropic{
		APIKey: apiKey,
		Model:  model,
		APIURL: anthropicAPIURL,
		client: newHTTPClient(),
	}
}

func (p *Anthropic) apiURL() string {
	if p.APIURL != "" {
		return p.APIURL
	}
	return anthropicAPIURL
}

// NewAnthropicFromEnv reads ANTHROPIC_API_KEY and optional ANTHROPIC_MODEL.
func NewAnthropicFromEnv() (*Anthropic, error) {
	apiKey := os.Getenv(anthropicAPIKeyEnv)
	if apiKey == "" {
		return nil, ErrMissingAPIKey
	}
	model := os.Getenv(anthropicModelEnv)
	return NewAnthropic(apiKey, model), nil
}

func (p *Anthropic) ID() string { return IDAnthropic }

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
		MaxTokens int       `json:"max_tokens"`
		System    string    `json:"system,omitempty"`
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
	err := postJSON(ctx, p.client, p.apiURL(), map[string]string{
		"x-api-key":         p.APIKey,
		"anthropic-version": anthropicVersion,
	}, request{
		Model:     model,
		MaxTokens: 4096,
		System:    req.SystemPrompt,
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
		return "", fmt.Errorf("anthropic: empty response")
	}
	return text, nil
}