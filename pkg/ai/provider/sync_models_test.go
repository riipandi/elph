package provider

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestModelConfigFromModelsDev(t *testing.T) {
	cfg := modelConfigFromModelsDev(ModelsDevModel{
		Name:      "Claude Sonnet 4.6",
		Reasoning: true,
		Modalities: ModelsDevModalities{
			Input: []string{"text", "image", "pdf"},
		},
		Limit: ModelsDevLimit{Context: 200000, Output: 16384},
		Cost: &ModelsDevCost{
			Input: 3, Output: 15, CacheRead: 0.3, CacheWrite: 3.75,
		},
		Provider: &ModelsDevModelProvider{NPM: "@ai-sdk/anthropic"},
	}, "@ai-sdk/openai-compatible")

	require.Equal(t, "Claude Sonnet 4.6", cfg.Name)
	require.True(t, cfg.Reasoning)
	require.Equal(t, []string{"text", "image"}, cfg.Input)
	require.Equal(t, 200000, cfg.ContextWindow)
	require.Equal(t, 16384, cfg.MaxTokens)
	require.Equal(t, APIAnthropicMessages, cfg.API)
	require.NotNil(t, cfg.Cost)
	require.Equal(t, 3.0, cfg.Cost.Input)
}

func TestUpdateModelsFromModelsDev(t *testing.T) {
	catalog := ModelsDevCatalog{
		Providers: map[string]ModelsDevProvider{
			"anthropic": {
				ID:   "anthropic",
				Name: "Anthropic",
				Models: map[string]ModelsDevModel{
					"claude-sonnet-4-20250514": {
						ID:   "claude-sonnet-4-20250514",
						Name: "Claude Sonnet 4",
						Modalities: ModelsDevModalities{
							Input: []string{"text", "image"},
						},
						Limit: ModelsDevLimit{Context: 200000, Output: 64000},
						Cost: &ModelsDevCost{
							Input: 3, Output: 15, CacheRead: 0.3, CacheWrite: 3.75,
						},
					},
				},
			},
		},
	}
	models := map[string]ModelsDevModel{}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/catalog.json":
			require.NoError(t, json.NewEncoder(w).Encode(catalog))
		case "/models.json":
			require.NoError(t, json.NewEncoder(w).Encode(models))
		default:
			http.NotFound(w, r)
		}
	}))
	defer srv.Close()

	dir := t.TempDir()
	initial := FileConfig{
		Name:    "Anthropic",
		BaseURL: "https://api.anthropic.com/v1",
		API:     APIAnthropicMessages,
		APIKey:  "env.ANTHROPIC_API_KEY",
		Models: []ModelConfig{
			{
				ID:            "claude-sonnet-4-20250514",
				Name:          "Old Name",
				ContextWindow: 128000,
				MaxTokens:     8192,
				Temperature:   ptrFloat(0.2),
			},
		},
	}
	raw, err := json.MarshalIndent(initial, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(dir, "anthropic.json"), append(raw, '\n'), 0o644))

	result, err := UpdateModelsFromModelsDev(UpdateModelsOptions{
		Dir: dir,
		Data: ModelsDevData{
			Catalog: catalog,
			Models:  models,
		},
	})
	require.NoError(t, err)
	require.Equal(t, []string{"anthropic.json"}, result.Updated)

	updated, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, updated.Providers, 1)
	model := updated.Providers[0].Models[0]
	require.Equal(t, "Claude Sonnet 4", model.Name)
	require.Equal(t, 200000, model.ContextWindow)
	require.Equal(t, 64000, model.MaxTokens)
	require.Equal(t, 0.2, model.Temperature)
	require.Equal(t, 3.0, model.Cost.Input)
}

func TestUpdateModelsFromModelsDevSyncsOpenCodeFromLiveAPI(t *testing.T) {
	catalog := ModelsDevCatalog{
		Providers: map[string]ModelsDevProvider{
			"opencode": {
				ID:   "opencode",
				Name: "OpenCode Zen",
				NPM:  "@ai-sdk/openai-compatible",
				Models: map[string]ModelsDevModel{
					"big-pickle": {
						ID:    "big-pickle",
						Name:  "Big Pickle",
						Limit: ModelsDevLimit{Context: 200000, Output: 32000},
					},
					"gpt-5.4": {
						ID:       "gpt-5.4",
						Name:     "GPT-5.4",
						Limit:    ModelsDevLimit{Context: 400000, Output: 128000},
						Provider: &ModelsDevModelProvider{NPM: "@ai-sdk/openai"},
					},
					"claude-sonnet-4-6": {
						ID:       "claude-sonnet-4-6",
						Name:     "Claude Sonnet 4.6",
						Limit:    ModelsDevLimit{Context: 1000000, Output: 64000},
						Provider: &ModelsDevModelProvider{NPM: "@ai-sdk/anthropic"},
					},
				},
			},
		},
	}

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/models", r.URL.Path)
		require.NoError(t, json.NewEncoder(w).Encode(OpenCodeModelsResponse{
			Object: "list",
			Data: []OpenCodeModelEntry{
				{ID: "big-pickle"},
				{ID: "gpt-5.4"},
				{ID: "claude-sonnet-4-6"},
			},
		}))
	}))
	defer srv.Close()

	dir := t.TempDir()
	initial := FileConfig{
		Name:    "OpenCode Zen",
		BaseURL: srv.URL,
		API:     APIOpenAICompletions,
		APIKey:  "env.OPENCODE_API_KEY",
		Models: []ModelConfig{
			{
				ID:            "big-pickle",
				Name:          "Big Pickle",
				ContextWindow: 128000,
				Temperature:   ptrFloat(0.2),
			},
			{ID: "stale-model", Name: "Stale"},
		},
	}
	raw, err := json.MarshalIndent(initial, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(dir, "opencode.json"), append(raw, '\n'), 0o644))

	result, err := UpdateModelsFromModelsDev(UpdateModelsOptions{
		Dir:        dir,
		HTTPClient: srv.Client(),
		Data: ModelsDevData{
			Catalog: catalog,
			Models:  map[string]ModelsDevModel{},
		},
	})
	require.NoError(t, err)
	require.Equal(t, []string{"opencode.json"}, result.Updated)
	require.Contains(t, result.Warnings[0], "stale-model")

	updated, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, updated.Providers, 1)
	require.Len(t, updated.Providers[0].Models, 3)

	byID := make(map[string]providerModelSnapshot, len(updated.Providers[0].Models))
	for _, model := range updated.Providers[0].Models {
		byID[model.ID] = providerModelSnapshot{
			Name:          model.Name,
			API:           model.API,
			ContextWindow: model.ContextWindow,
			Temperature:   model.Temperature,
		}
	}
	require.Equal(t, 200000, byID["big-pickle"].ContextWindow)
	require.Equal(t, 0.2, byID["big-pickle"].Temperature)
	require.Equal(t, "GPT-5.4", byID["gpt-5.4"].Name)
	require.Equal(t, APIOpenAICompletions, byID["gpt-5.4"].API)
	require.Equal(t, APIAnthropicMessages, byID["claude-sonnet-4-6"].API)
}

type providerModelSnapshot struct {
	Name          string
	API           API
	ContextWindow int
	Temperature   float64
}

func TestUpdateModelsFromModelsDevAddsMissingCatalogModels(t *testing.T) {
	catalog := ModelsDevCatalog{
		Providers: map[string]ModelsDevProvider{
			"anthropic": {
				ID:   "anthropic",
				Name: "Anthropic",
				Models: map[string]ModelsDevModel{
					"claude-sonnet-4-20250514": {
						ID:    "claude-sonnet-4-20250514",
						Name:  "Claude Sonnet 4",
						Limit: ModelsDevLimit{Context: 200000, Output: 64000},
					},
					"claude-opus-4-20250514": {
						ID:    "claude-opus-4-20250514",
						Name:  "Claude Opus 4",
						Limit: ModelsDevLimit{Context: 200000, Output: 32000},
					},
				},
			},
		},
	}

	dir := t.TempDir()
	initial := FileConfig{
		Name:    "Anthropic",
		BaseURL: "https://api.anthropic.com/v1",
		API:     APIAnthropicMessages,
		APIKey:  "env.ANTHROPIC_API_KEY",
		Models: []ModelConfig{
			{ID: "claude-sonnet-4-20250514", Name: "Old Name"},
		},
	}
	raw, err := json.MarshalIndent(initial, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(dir, "anthropic.json"), append(raw, '\n'), 0o644))

	result, err := UpdateModelsFromModelsDev(UpdateModelsOptions{
		Dir: dir,
		Data: ModelsDevData{
			Catalog: catalog,
			Models:  map[string]ModelsDevModel{},
		},
	})
	require.NoError(t, err)
	require.Equal(t, []string{"anthropic.json"}, result.Updated)

	updated, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, updated.Providers[0].Models, 2)
}

func TestUpdateModelsFromModelsDevSkipsUnknownProvider(t *testing.T) {
	dir := t.TempDir()
	cfg := FileConfig{
		BaseURL: "https://example.com/v1",
		API:     APIOpenAICompletions,
		APIKey:  "test",
		Models:  []ModelConfig{{ID: "m1"}},
	}
	raw, err := json.MarshalIndent(cfg, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(dir, "custom.json"), append(raw, '\n'), 0o644))

	result, err := UpdateModelsFromModelsDev(UpdateModelsOptions{
		Dir: dir,
		Data: ModelsDevData{
			Catalog: ModelsDevCatalog{Providers: map[string]ModelsDevProvider{}},
			Models:  map[string]ModelsDevModel{},
		},
	})
	require.NoError(t, err)
	require.Empty(t, result.Updated)
	require.Contains(t, result.Skipped[0], "custom.json")
}

func ptrFloat(v float64) *float64 {
	return &v
}
