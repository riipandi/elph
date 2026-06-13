package ai

import (
	"strings"

	"github.com/riipandi/elph/internal/fuzzy"
	"github.com/riipandi/elph/pkg/ai/provider"
)

// ModelRef is the canonical provider/model selector string.
func ModelRef(providerID, modelID string) string {
	return providerID + "/" + modelID
}

// MatchModel finds a model by provider/model ref, id, or fuzzy name match.
func MatchModel(catalog provider.Catalog, query string) (provider.RegisteredProvider, provider.ResolvedModel, bool) {
	query = strings.TrimSpace(query)
	if query == "" {
		return provider.RegisteredProvider{}, provider.ResolvedModel{}, false
	}

	if providerID, modelID, ok := strings.Cut(query, "/"); ok {
		reg, ok := catalog.Provider(providerID)
		if !ok {
			return provider.RegisteredProvider{}, provider.ResolvedModel{}, false
		}
		if model, ok := matchProviderModel(reg, modelID); ok {
			return reg, model, true
		}
		return provider.RegisteredProvider{}, provider.ResolvedModel{}, false
	}

	bestScore := -1
	var bestProvider provider.RegisteredProvider
	var bestModel provider.ResolvedModel
	lower := strings.ToLower(query)

	for _, reg := range catalog.Providers {
		for _, model := range reg.Models {
			if exactModelMatch(lower, reg.ID, model) {
				return reg, model, true
			}
			score := modelMatchScore(lower, reg.ID, model)
			if score > bestScore {
				bestScore = score
				bestProvider = reg
				bestModel = model
			}
		}
	}
	if bestScore < 0 {
		return provider.RegisteredProvider{}, provider.ResolvedModel{}, false
	}
	return bestProvider, bestModel, true
}

func matchProviderModel(reg provider.RegisteredProvider, query string) (provider.ResolvedModel, bool) {
	query = strings.TrimSpace(query)
	if query == "" {
		if len(reg.Models) > 0 {
			return reg.Models[0], true
		}
		return provider.ResolvedModel{}, false
	}
	lower := strings.ToLower(query)
	for _, model := range reg.Models {
		if exactModelMatch(lower, reg.ID, model) {
			return model, true
		}
	}
	for _, model := range reg.Models {
		if modelMatchScore(lower, reg.ID, model) >= 0 {
			return model, true
		}
	}
	return provider.ResolvedModel{}, false
}

func exactModelMatch(lowerQuery, providerID string, model provider.ResolvedModel) bool {
	return lowerQuery == strings.ToLower(model.ID) ||
		lowerQuery == strings.ToLower(model.Name) ||
		lowerQuery == strings.ToLower(ModelRef(providerID, model.ID))
}

func modelMatchScore(lowerQuery, providerID string, model provider.ResolvedModel) int {
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
