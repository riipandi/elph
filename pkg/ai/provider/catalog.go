package provider

import (
	"fmt"
	"strings"

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

// FirstConfigured returns the first provider with a configured API key and its first model.
func (c Catalog) FirstConfigured() (RegisteredProvider, ResolvedModel, bool) {
	for _, provider := range c.Providers {
		if !IsConfigured(provider.Config.APIKey) {
			continue
		}
		if len(provider.Models) == 0 {
			continue
		}
		return provider, provider.Models[0], true
	}
	return RegisteredProvider{}, ResolvedModel{}, false
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

	switch model.API {
	case APIOpenAICompletions:
		return NewOpenAICompatible(OpenAIOptions{
			ID:           provider.ID,
			APIKey:       apiKey,
			BaseURL:      model.BaseURL,
			DefaultModel: model.ID,
			Headers:      headers,
			AuthHeader:   provider.Config.AuthHeader,
			MaxTokens:    model.MaxTokens,
			Temperature:  model.Temperature,
		}), nil
	case APIAnthropicMessages:
		return NewAnthropic(AnthropicOptions{
			ID:          provider.ID,
			APIKey:      apiKey,
			Model:       model.ID,
			BaseURL:     model.BaseURL,
			Headers:     headers,
			MaxTokens:   model.MaxTokens,
			Temperature: model.Temperature,
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
