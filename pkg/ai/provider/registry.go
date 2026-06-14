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
func Resolve() Config {
	return ResolveActive("", "")
}

// ResolveActive loads providers and resolves the active provider/model.
// Priority: ELPH_PROVIDER/ELPH_MODEL env, saved provider/model, then the first
// configured provider. ELPH_MODEL still applies when only the model env is set.
func ResolveActive(savedProviderID, savedModelID string) Config {
	catalog, err := LoadCatalog("")
	if err != nil {
		return Config{Catalog: catalog}
	}
	return resolveCatalog(catalog, savedProviderID, savedModelID)
}

func resolveCatalog(catalog Catalog, savedProviderID, savedModelID string) Config {
	envProvider := strings.TrimSpace(os.Getenv(elphProviderEnv))
	envModel := strings.TrimSpace(os.Getenv(elphModelEnv))

	if envProvider != "" {
		provider, ok := catalog.Provider(envProvider)
		if !ok {
			return Config{Catalog: catalog}
		}
		model, ok := pickModel(provider, envModel)
		if !ok {
			return Config{Catalog: catalog}
		}
		return buildConfig(catalog, provider, model)
	}

	if savedProvider := strings.TrimSpace(savedProviderID); savedProvider != "" {
		if provider, ok := catalog.Provider(savedProvider); ok {
			model, ok := pickModel(provider, strings.TrimSpace(savedModelID))
			if !ok && len(provider.Models) > 0 {
				model, ok = provider.Models[0], true
			}
			if ok {
				if cfg := buildConfig(catalog, provider, model); cfg.Provider != nil {
					return cfg
				}
			}
		}
	}

	provider, model, ok := catalog.FirstConfigured()
	if !ok {
		return Config{Catalog: catalog}
	}
	if envModel != "" {
		if picked, ok := pickModel(provider, envModel); ok {
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
