// Package openaicompat provides OpenAI-compatible chat completion adapters.
package openaicompat

import (
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
)

const (
	// Name is the default provider identifier for compatible endpoints.
	Name = "openai-compat"
)

// Options configures an OpenAI-compatible provider.
type Options = elphopenai.Options

// New builds a provider.Provider with compat-specific hooks.
func New(opts Options) provider.Provider {
	if opts.ID == "" {
		opts.ID = Name
	}
	opts.Hooks = compatHooks()
	return elphopenai.New(opts)
}
