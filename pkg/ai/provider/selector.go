package provider

import (
	"strings"

	"github.com/riipandi/elph/internal/fuzzy"
)

// SelectorGroup is a provider bucket in the model picker.
type SelectorGroup struct {
	ProviderID   string
	ProviderName string
	Models       []ResolvedModel
}

// BuildSelectorGroups returns filtered provider groups and a flat model list.
func BuildSelectorGroups(catalog Catalog, query string) ([]SelectorGroup, []ResolvedModel) {
	query = strings.TrimSpace(query)
	groups := make([]SelectorGroup, 0, len(catalog.Providers))
	flat := make([]ResolvedModel, 0)

	for _, reg := range catalog.Providers {
		matched := make([]ResolvedModel, 0, len(reg.Models))
		for _, model := range reg.Models {
			if selectorMatches(query, reg.ID, model) {
				matched = append(matched, model)
			}
		}
		if len(matched) == 0 {
			continue
		}

		providerName := reg.Config.Name
		if providerName == "" {
			providerName = reg.ID
		}
		groups = append(groups, SelectorGroup{
			ProviderID:   reg.ID,
			ProviderName: providerName,
			Models:       matched,
		})
		flat = append(flat, matched...)
	}
	return groups, flat
}

// SelectorPickIndex returns the best flat index for the active model.
// FlattenSelectorGroups returns models for a provider filter. An empty providerID
// includes models from every group.
func FlattenSelectorGroups(groups []SelectorGroup, providerID string) []ResolvedModel {
	if providerID == "" {
		flat := make([]ResolvedModel, 0)
		for _, group := range groups {
			flat = append(flat, group.Models...)
		}
		return flat
	}
	for _, group := range groups {
		if group.ProviderID == providerID {
			return append([]ResolvedModel(nil), group.Models...)
		}
	}
	return nil
}

// CycleProviderFilter advances the provider filter through all providers and back
// to the unfiltered view.
func CycleProviderFilter(current string, delta int, groups []SelectorGroup) string {
	if len(groups) == 0 {
		return ""
	}
	slots := make([]string, len(groups)+1)
	slots[0] = ""
	for i, group := range groups {
		slots[i+1] = group.ProviderID
	}
	idx := 0
	for i, id := range slots {
		if id == current {
			idx = i
			break
		}
	}
	idx = (idx + delta + len(slots)) % len(slots)
	return slots[idx]
}

// NormalizeProviderFilter keeps the filter valid for the current group set.
func NormalizeProviderFilter(providerID string, groups []SelectorGroup) string {
	if providerID == "" {
		return ""
	}
	for _, group := range groups {
		if group.ProviderID == providerID {
			return providerID
		}
	}
	return ""
}

func SelectorPickIndex(flat []ResolvedModel, providerID, modelID string) int {
	for i, model := range flat {
		if model.ProviderID == providerID && model.ID == modelID {
			return i
		}
	}
	if len(flat) > 0 {
		return 0
	}
	return 0
}

func selectorMatches(query, providerID string, model ResolvedModel) bool {
	if query == "" {
		return true
	}
	lower := strings.ToLower(query)
	scores := []int{
		fuzzy.Score(lower, model.ID),
		fuzzy.Score(lower, model.Name),
		fuzzy.Score(lower, ModelRef(providerID, model.ID)),
		fuzzy.Score(lower, providerID),
		fuzzy.Score(lower, model.ProviderName),
	}
	for _, score := range scores {
		if score >= 0 {
			return true
		}
	}
	return false
}
