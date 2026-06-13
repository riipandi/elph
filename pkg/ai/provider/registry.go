package provider

import (
	"os"
	"strings"
)

const (
	elphProviderEnv = "ELPH_PROVIDER"
	elphModelEnv    = "ELPH_MODEL"
)

// Config holds the resolved default provider and display metadata.
type Config struct {
	Provider      Provider
	ModelID       string
	ModelName     string
	ProviderID    string
	ProviderName  string
	ContextWindow int
	MaxTokens     int
	Catalog       Catalog
}

// Resolve loads user-defined providers and picks the active provider/model.
// ELPH_PROVIDER and ELPH_MODEL select a specific entry; otherwise the first
// configured provider is used.
func Resolve() Config {
	catalog, err := LoadCatalog("")
	if err != nil {
		return Config{Catalog: catalog}
	}
	return resolveCatalog(catalog)
}

func resolveCatalog(catalog Catalog) Config {
	providerID := strings.TrimSpace(os.Getenv(elphProviderEnv))
	modelID := strings.TrimSpace(os.Getenv(elphModelEnv))

	if providerID != "" {
		provider, ok := catalog.Provider(providerID)
		if !ok {
			return Config{Catalog: catalog}
		}
		model, ok := pickModel(provider, modelID)
		if !ok {
			return Config{Catalog: catalog}
		}
		return buildConfig(catalog, provider, model)
	}

	provider, model, ok := catalog.FirstConfigured()
	if !ok {
		return Config{Catalog: catalog}
	}
	if modelID != "" {
		if picked, ok := pickModel(provider, modelID); ok {
			model = picked
		}
	}
	return buildConfig(catalog, provider, model)
}

func pickModel(provider RegisteredProvider, modelID string) (ResolvedModel, bool) {
	if len(provider.Models) == 0 {
		return ResolvedModel{}, false
	}
	if modelID == "" {
		return provider.Models[0], true
	}
	for _, model := range provider.Models {
		if model.ID == modelID || model.Name == modelID {
			return model, true
		}
	}
	return ResolvedModel{}, false
}

func buildConfig(catalog Catalog, provider RegisteredProvider, model ResolvedModel) Config {
	cfg, err := SelectModel(catalog, provider, model)
	if err != nil {
		return Config{Catalog: catalog}
	}
	return cfg
}

// SelectModel resolves credentials and builds a runtime config for provider/model.
func SelectModel(catalog Catalog, provider RegisteredProvider, model ResolvedModel) (Config, error) {
	runtimeProvider, err := NewProvider(provider, model)
	if err != nil {
		return Config{Catalog: catalog}, err
	}
	return Config{
		Provider:      runtimeProvider,
		ModelID:       model.ID,
		ModelName:     model.Name,
		ProviderID:    provider.ID,
		ProviderName:  model.ProviderName,
		ContextWindow: model.ContextWindow,
		MaxTokens:     model.MaxTokens,
		Catalog:       catalog,
	}, nil
}
