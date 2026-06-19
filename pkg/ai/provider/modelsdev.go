package provider

import (
	"context"
	"fmt"
	"strings"

	"resty.dev/v3"

	"github.com/riipandi/elph/pkg/ai/utils"
)

const (
	ModelsDevCatalogURL = "https://models.dev/catalog.json"
	ModelsDevModelsURL  = "https://models.dev/models.json"
)

// ModelsDevCatalog is the provider catalog published by models.dev.
type ModelsDevCatalog struct {
	Models    map[string]ModelsDevModel    `json:"models"`
	Providers map[string]ModelsDevProvider `json:"providers"`
}

// ModelsDevProvider describes one upstream provider in the catalog.
type ModelsDevProvider struct {
	ID     string                    `json:"id"`
	Name   string                    `json:"name"`
	NPM    string                    `json:"npm"`
	API    string                    `json:"api"`
	Env    []string                  `json:"env"`
	Models map[string]ModelsDevModel `json:"models"`
}

// ModelsDevModel is model metadata from models.dev.
type ModelsDevModel struct {
	ID         string                  `json:"id"`
	Name       string                  `json:"name"`
	Reasoning  bool                    `json:"reasoning"`
	Modalities ModelsDevModalities     `json:"modalities"`
	Limit      ModelsDevLimit          `json:"limit"`
	Cost       *ModelsDevCost          `json:"cost,omitempty"`
	Provider   *ModelsDevModelProvider `json:"provider,omitempty"`
}

type ModelsDevModalities struct {
	Input []string `json:"input"`
}

type ModelsDevLimit struct {
	Context int `json:"context"`
	Output  int `json:"output"`
}

// ModelsDevCost uses models.dev snake_case field names.
type ModelsDevCost struct {
	Input      float64 `json:"input"`
	Output     float64 `json:"output"`
	CacheRead  float64 `json:"cache_read"`
	CacheWrite float64 `json:"cache_write"`
}

type ModelsDevModelProvider struct {
	NPM string `json:"npm"`
	API string `json:"api"`
}

// ModelsDevData holds fetched catalog and global model indexes.
type ModelsDevData struct {
	Catalog ModelsDevCatalog
	Models  map[string]ModelsDevModel
}

// FetchModelsDev downloads catalog.json and models.json from models.dev.
func FetchModelsDev(ctx context.Context, client *resty.Client) (ModelsDevData, error) {
	if client == nil {
		client = utils.NewHTTPClient()
	}

	var catalog ModelsDevCatalog
	if err := utils.GetJSON(ctx, client, ModelsDevCatalogURL, &catalog); err != nil {
		return ModelsDevData{}, fmt.Errorf("fetch catalog: %w", err)
	}

	var models map[string]ModelsDevModel
	if err := utils.GetJSON(ctx, client, ModelsDevModelsURL, &models); err != nil {
		return ModelsDevData{}, fmt.Errorf("fetch models: %w", err)
	}

	if catalog.Providers == nil {
		catalog.Providers = map[string]ModelsDevProvider{}
	}
	if catalog.Models == nil {
		catalog.Models = map[string]ModelsDevModel{}
	}
	if models == nil {
		models = map[string]ModelsDevModel{}
	}

	return ModelsDevData{Catalog: catalog, Models: models}, nil
}

func (d ModelsDevData) lookupModel(providerID, modelID string) (ModelsDevModel, bool) {
	if providerID != "" {
		if provider, ok := d.Catalog.Providers[providerID]; ok {
			if model, ok := provider.Models[modelID]; ok {
				return model, true
			}
		}
		ref := providerID + "/" + modelID
		if model, ok := d.Catalog.Models[ref]; ok {
			return model, true
		}
		if model, ok := d.Models[ref]; ok {
			return model, true
		}
	}
	if model, ok := d.Catalog.Models[modelID]; ok {
		return model, true
	}
	if model, ok := d.Models[modelID]; ok {
		return model, true
	}
	return ModelsDevModel{}, false
}

func modelConfigFromModelsDev(src ModelsDevModel, providerNPM string) ModelConfig {
	out := ModelConfig{
		Name:      strings.TrimSpace(src.Name),
		Reasoning: src.Reasoning,
		Input:     filterInputModalities(src.Modalities.Input),
	}
	if src.Limit.Context > 0 {
		out.ContextWindow = src.Limit.Context
	}
	if src.Limit.Output > 0 {
		out.MaxTokens = src.Limit.Output
	}
	if src.Cost != nil {
		out.Cost = &Cost{
			Input:      src.Cost.Input,
			Output:     src.Cost.Output,
			CacheRead:  src.Cost.CacheRead,
			CacheWrite: src.Cost.CacheWrite,
		}
	}
	npm := ""
	if src.Provider != nil {
		npm = strings.TrimSpace(src.Provider.NPM)
	}
	if npm == "" {
		npm = strings.TrimSpace(providerNPM)
	}
	if npm != "" {
		if api, ok := npmToAPI(npm); ok {
			out.API = api
		}
	}
	return out
}

func npmToAPI(npm string) (API, bool) {
	switch strings.TrimSpace(npm) {
	case "@ai-sdk/openai", "@ai-sdk/openai-compatible":
		return APIOpenAICompletions, true
	case "@ai-sdk/anthropic":
		return APIAnthropicMessages, true
	default:
		return "", false
	}
}

func filterInputModalities(input []string) []string {
	if len(input) == 0 {
		return []string{"text"}
	}
	out := make([]string, 0, len(input))
	seen := make(map[string]struct{}, len(input))
	for _, modality := range input {
		switch modality {
		case "text", "image":
			if _, ok := seen[modality]; ok {
				continue
			}
			seen[modality] = struct{}{}
			out = append(out, modality)
		}
	}
	if len(out) == 0 {
		return []string{"text"}
	}
	return out
}

func mergeModelConfig(existing ModelConfig, fresh ModelConfig) ModelConfig {
	merged := existing
	if fresh.Name != "" {
		merged.Name = fresh.Name
	}
	merged.Reasoning = fresh.Reasoning
	if len(fresh.Input) > 0 {
		merged.Input = fresh.Input
	}
	if fresh.ContextWindow > 0 {
		merged.ContextWindow = fresh.ContextWindow
	}
	if fresh.MaxTokens > 0 {
		merged.MaxTokens = fresh.MaxTokens
	}
	if fresh.Cost != nil {
		merged.Cost = fresh.Cost
	}
	if fresh.API != "" {
		merged.API = fresh.API
	}
	// Preserve user/provider thinking controls set outside models.dev sync.
	// ThinkingLevelMap, Compat, Temperature, TopP, Headers, and BaseURL stay on existing.
	return merged
}

func mergeModelConfigWithTemplate(providerID string, existing, fresh ModelConfig) ModelConfig {
	merged := mergeModelConfig(existing, fresh)
	if tmpl, ok := thinkingTemplateModel(providerID, merged.ID); ok {
		merged = backfillModelThinking(merged, tmpl)
	}
	return merged
}
