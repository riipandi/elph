// Package google provides a Google GenAI adapter stub for future Gemini support.
package google

import (
	"context"
	"fmt"

	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

const (
	// Name is the default provider identifier.
	Name = "google"
)

// Options configures a Google GenAI provider.
type Options struct {
	ID      string
	APIKey  string
	Model   string
	BaseURL string
}

type languageModel struct {
	opts Options
}

// New builds a provider.Provider placeholder until GenAI completion is wired.
func New(opts Options) provider.Provider {
	return &languageModel{opts: opts}
}

func (p *languageModel) ID() string {
	if p.opts.ID == "" {
		return Name
	}
	return p.opts.ID
}

func (p *languageModel) Complete(context.Context, provider.TurnRequest) (provider.TurnResult, error) {
	return provider.TurnResult{}, fmt.Errorf("%s: google genai provider is not implemented yet", p.ID())
}
