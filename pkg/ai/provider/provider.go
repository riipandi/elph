package provider

import (
	"context"
	"errors"
)

// Common provider identifiers used in UI and configuration.
const (
	IDAnthropic = "anthropic"
	IDOpenAI    = "openai"
	IDDeepSeek  = "deepseek"
)

// TurnRequest is a single non-streaming completion request.
type TurnRequest struct {
	SystemPrompt string
	UserPrompt   string
	Model        string
}

// Provider completes one agent turn against an upstream model API.
type Provider interface {
	ID() string
	Complete(ctx context.Context, req TurnRequest) (string, error)
}

// ErrMissingAPIKey reports that no provider credentials are configured.
var ErrMissingAPIKey = errors.New("provider: missing API key")