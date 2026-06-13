package provider

import (
	"fmt"
	"strings"

	"github.com/riipandi/elph/internal/fuzzy"
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

	headers, err := ResolveHeaders(mergeStringMaps(provider.Config.Headers, model.Headers))
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
		}), nil
	case APIAnthropicMessages:
		return NewAnthropic(AnthropicOptions{
			ID:        provider.ID,
			APIKey:    apiKey,
			Model:     model.ID,
			BaseURL:   model.BaseURL,
			Headers:   headers,
			MaxTokens: model.MaxTokens,
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

// ModelRef is the canonical provider/model selector string.
func ModelRef(providerID, modelID string) string {
	return providerID + "/" + modelID
}

// MatchModel finds a model by provider/model ref, id, or fuzzy name match.
func (c Catalog) MatchModel(query string) (RegisteredProvider, ResolvedModel, bool) {
	query = strings.TrimSpace(query)
	if query == "" {
		return RegisteredProvider{}, ResolvedModel{}, false
	}

	if providerID, modelID, ok := strings.Cut(query, "/"); ok {
		provider, ok := c.Provider(providerID)
		if !ok {
			return RegisteredProvider{}, ResolvedModel{}, false
		}
		if model, ok := matchProviderModel(provider, modelID); ok {
			return provider, model, true
		}
		return RegisteredProvider{}, ResolvedModel{}, false
	}

	bestScore := -1
	var bestProvider RegisteredProvider
	var bestModel ResolvedModel
	lower := strings.ToLower(query)

	for _, provider := range c.Providers {
		for _, model := range provider.Models {
			if exactModelMatch(lower, provider.ID, model) {
				return provider, model, true
			}
			score := modelMatchScore(lower, provider.ID, model)
			if score > bestScore {
				bestScore = score
				bestProvider = provider
				bestModel = model
			}
		}
	}
	if bestScore < 0 {
		return RegisteredProvider{}, ResolvedModel{}, false
	}
	return bestProvider, bestModel, true
}

func matchProviderModel(provider RegisteredProvider, query string) (ResolvedModel, bool) {
	query = strings.TrimSpace(query)
	if query == "" {
		if len(provider.Models) > 0 {
			return provider.Models[0], true
		}
		return ResolvedModel{}, false
	}
	lower := strings.ToLower(query)
	for _, model := range provider.Models {
		if exactModelMatch(lower, provider.ID, model) {
			return model, true
		}
	}
	for _, model := range provider.Models {
		if modelMatchScore(lower, provider.ID, model) >= 0 {
			return model, true
		}
	}
	return ResolvedModel{}, false
}

func exactModelMatch(lowerQuery, providerID string, model ResolvedModel) bool {
	return lowerQuery == strings.ToLower(model.ID) ||
		lowerQuery == strings.ToLower(model.Name) ||
		lowerQuery == strings.ToLower(ModelRef(providerID, model.ID))
}

func modelMatchScore(lowerQuery, providerID string, model ResolvedModel) int {
	scores := []int{
		fuzzy.Score(lowerQuery, model.ID),
		fuzzy.Score(lowerQuery, model.Name),
		fuzzy.Score(lowerQuery, ModelRef(providerID, model.ID)),
		fuzzy.Score(lowerQuery, providerID),
	}
	best := -1
	for _, score := range scores {
		if score > best {
			best = score
		}
	}
	return best
}
