// Package openai provides an OpenAI chat completions adapter for Elph providers.
package openai

import (
	"strings"

	openaisdk "github.com/openai/openai-go/v3"
	"github.com/openai/openai-go/v3/option"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/ai/providers/internal/httpheaders"
	"github.com/riipandi/elph/pkg/ai/utils"
)

const (
	// Name is the default provider identifier.
	Name = "openai"
	// DefaultURL is the default OpenAI API base URL.
	DefaultURL = "https://api.openai.com/v1"
)

// Options configures an OpenAI chat completions provider.
type Options struct {
	ID           string
	APIKey       string
	BaseURL      string
	DefaultModel string
	Headers      map[string]string
	AuthHeader   bool
	MaxTokens    int
	Temperature  float64
	TopP         float64
	UserAgent    string
	Hooks        Hooks
}

// New builds a provider.Provider backed by openai-go.
func New(opts Options) provider.Provider {
	baseURL := opts.BaseURL
	if baseURL == "" {
		baseURL = DefaultURL
	}
	baseURL = strings.TrimRight(baseURL, "/")

	maxTokens := provider.MaxTokensOrDefault(opts.MaxTokens)
	hooks := opts.Hooks
	if hooks.ChatMessages == nil && hooks.PrepareParams == nil && hooks.ChoiceReasoning == nil && hooks.StreamReasoning == nil {
		hooks = DefaultHooks()
	} else {
		defaults := DefaultHooks()
		if hooks.ChatMessages == nil {
			hooks.ChatMessages = defaults.ChatMessages
		}
		if hooks.ChoiceReasoning == nil {
			hooks.ChoiceReasoning = defaults.ChoiceReasoning
		}
		if hooks.StreamReasoning == nil {
			hooks.StreamReasoning = defaults.StreamReasoning
		}
	}

	clientOpts := []option.RequestOption{
		option.WithBaseURL(baseURL),
		option.WithHTTPClient(utils.NewHTTPClient()),
	}
	if opts.APIKey != "" {
		clientOpts = append(clientOpts, option.WithAPIKey(opts.APIKey))
	}
	resolved := httpheaders.ResolveHeaders(opts.Headers, opts.UserAgent, httpheaders.DefaultUserAgent(""))
	for key, value := range resolved {
		clientOpts = append(clientOpts, option.WithHeader(key, value))
	}
	if opts.AuthHeader && opts.APIKey == "" && !hasHeader(opts.Headers, "Authorization") {
		clientOpts = append(clientOpts, option.WithHeader("Authorization", "Bearer "))
	}

	return &languageModel{
		opts: Options{
			ID:           opts.ID,
			APIKey:       opts.APIKey,
			BaseURL:      baseURL,
			DefaultModel: opts.DefaultModel,
			Headers:      opts.Headers,
			AuthHeader:   opts.AuthHeader,
			MaxTokens:    maxTokens,
			Temperature:  opts.Temperature,
			TopP:         opts.TopP,
			UserAgent:    opts.UserAgent,
		},
		client: openaisdk.NewClient(clientOpts...),
		hooks:  hooks,
	}
}

func hasHeader(headers map[string]string, key string) bool {
	for k := range headers {
		if strings.EqualFold(k, key) {
			return true
		}
	}
	return false
}
