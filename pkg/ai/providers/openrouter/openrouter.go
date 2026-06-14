// Package openrouter provides an OpenRouter adapter built on openaicompat.
package openrouter

import (
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
)

const (
	// Name is the default provider identifier for OpenRouter.
	Name = "openrouter"
	// DefaultURL is the default OpenRouter API base URL.
	DefaultURL = "https://openrouter.ai/api/v1"
)

// Options configures an OpenRouter provider.
type Options = elphopenai.Options

// New builds a provider.Provider with OpenRouter-specific hooks.
func New(opts Options) provider.Provider {
	if opts.ID == "" {
		opts.ID = Name
	}
	if opts.BaseURL == "" {
		opts.BaseURL = DefaultURL
	}
	opts.Hooks = openRouterHooks()
	return elphopenai.New(opts)
}
