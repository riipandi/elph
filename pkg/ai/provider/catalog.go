package provider

import (
	"fmt"
	"strings"

	elphant "github.com/riipandi/elph/pkg/ai/providers/anthropic"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
	elphcompat "github.com/riipandi/elph/pkg/ai/providers/openaicompat"
	elphor "github.com/riipandi/elph/pkg/ai/providers/openrouter"
	"github.com/riipandi/elph/pkg/ai/utils"
)

// Catalog holds all user-defined providers loaded from disk.
type Catalog struct {
	Dir       string
	Providers []RegisteredProvider
	Errors    []error
}

// Provider returns a registered provider by id.
func (c Catalog) Provider(id string) (RegisteredProvider, bool) {
	for _, provider := range c.Providers {
		if provider.ID == id {
			return provider, true
		}
	}
	return RegisteredProvider{}, false
}

// Model returns a resolved model by provider id and model id.
func (c Catalog) Model(providerID, modelID string) (ResolvedModel, bool) {
	provider, ok := c.Provider(providerID)
	if !ok {
		return ResolvedModel{}, false
	}
	for _, model := range provider.Models {
		if model.ID == modelID {
			return model, true
		}
	}
	return ResolvedModel{}, false
}

// FirstConfigured returns the first enabled provider with a configured API key
// and its first enabled model.
func (c Catalog) FirstConfigured() (RegisteredProvider, ResolvedModel, bool) {
	for _, provider := range c.Providers {
		if !ProviderConfigEnabled(provider.Config) {
			continue
		}
		if !IsConfigured(provider.Config.APIKey) {
			continue
		}
		if model, ok := FirstEnabledModel(provider); ok {
			return provider, model, true
		}
	}
	return RegisteredProvider{}, ResolvedModel{}, false
}

// FirstEnabledModel returns the first enabled model for a provider.
func FirstEnabledModel(provider RegisteredProvider) (ResolvedModel, bool) {
	for _, model := range provider.Models {
		if model.Enabled {
			return model, true
		}
	}
	return ResolvedModel{}, false
}

// EnabledModelCount returns how many models are enabled for a provider.
func EnabledModelCount(provider RegisteredProvider) int {
	n := 0
	for _, model := range provider.Models {
		if model.Enabled {
			n++
		}
	}
	return n
}

// NewProvider builds a runtime Provider for the given registered provider and model.
func NewProvider(provider RegisteredProvider, model ResolvedModel) (Provider, error) {
	apiKey, err := ResolveValue(provider.Config.APIKey)
	if err != nil {
		return nil, fmt.Errorf("provider %q: resolve apiKey: %w", provider.ID, err)
	}
	if strings.TrimSpace(apiKey) == "" {
		return nil, ErrMissingAPIKey
	}

	headers, err := ResolveHeaders(utils.MergeStringMaps(provider.Config.Headers, model.Headers))
	if err != nil {
		return nil, fmt.Errorf("provider %q: %w", provider.ID, err)
	}

	openAIOpts := elphopenai.Options{
		ID:           provider.ID,
		APIKey:       apiKey,
		BaseURL:      model.BaseURL,
		DefaultModel: model.ID,
		Headers:      headers,
		AuthHeader:   provider.Config.AuthHeader,
		MaxTokens:    model.MaxTokens,
		Temperature:  model.Temperature,
		TopP:         model.TopP,
	}

	switch model.API {
	case APIOpenAICompletions:
		if model.Compat.ThinkingFormat == string(ThinkingFormatOpenRouter) {
			return elphor.New(openAIOpts), nil
		}
		return elphcompat.New(openAIOpts), nil
	case APIAnthropicMessages:
		return elphant.New(elphant.Options{
			ID:          provider.ID,
			APIKey:      apiKey,
			Model:       model.ID,
			BaseURL:     model.BaseURL,
			Headers:     headers,
			MaxTokens:   model.MaxTokens,
			Temperature: model.Temperature,
			TopP:        model.TopP,
		}), nil
	default:
		return nil, fmt.Errorf("provider %q: unsupported api %q", provider.ID, model.API)
	}
}

// AllModels returns every model across all providers in catalog order.
func (c Catalog) AllModels() []ResolvedModel {
	total := 0
	for _, provider := range c.Providers {
		total += len(provider.Models)
	}
	out := make([]ResolvedModel, 0, total)
	for _, provider := range c.Providers {
		out = append(out, provider.Models...)
	}
	return out
}
