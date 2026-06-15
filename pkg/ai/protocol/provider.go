package protocol

import (
	"context"
	"errors"
	"time"
)

// TurnRequest is a completion request to an upstream model API.
type TurnRequest struct {
	SystemPrompt       string
	UserPrompt         string
	Model              string
	Thinking           ThinkingConfig
	Compat             Compat
	Stream             *TurnStream
	Messages           []ChatMessage
	Tools              []ToolDefinition
	StreamStallTimeout time.Duration
}

// Provider completes one agent turn against an upstream model API.
type Provider interface {
	ID() string
	Complete(ctx context.Context, req TurnRequest) (TurnResult, error)
}

// ErrMissingAPIKey reports that no provider credentials are configured.
var ErrMissingAPIKey = errors.New("provider: missing API key")
