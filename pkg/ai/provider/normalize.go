package provider

import "fmt"

const (
	defaultContextWindow = 128000
	defaultMaxTokens     = 16384
)

func normalizeProvider(id string, cfg FileConfig) (RegisteredProvider, error) {
	if cfg.API == "" {
		return RegisteredProvider{}, fmt.Errorf("provider %q: missing api", id)
	}
	if cfg.BaseURL == "" {
		return RegisteredProvider{}, fmt.Errorf("provider %q: missing baseUrl", id)
	}
	if len(cfg.Models) == 0 {
		return RegisteredProvider{}, fmt.Errorf("provider %q: missing models", id)
	}

	name := cfg.Name
	if name == "" {
		name = id
	}

	models := make([]ResolvedModel, 0, len(cfg.Models))
	for _, model := range cfg.Models {
		resolved, err := normalizeModel(id, name, cfg, model)
		if err != nil {
			return RegisteredProvider{}, err
		}
		models = append(models, resolved)
	}

	return RegisteredProvider{
		ID:     id,
		Config: cfg,
		Models: models,
	}, nil
}

func normalizeModel(providerID, providerName string, cfg FileConfig, model ModelConfig) (ResolvedModel, error) {
	if model.ID == "" {
		return ResolvedModel{}, fmt.Errorf("provider %q: model missing id", providerID)
	}

	name := model.Name
	if name == "" {
		name = model.ID
	}

	api := model.API
	if api == "" {
		api = cfg.API
	}
	if api == "" {
		return ResolvedModel{}, fmt.Errorf("provider %q model %q: missing api", providerID, model.ID)
	}

	baseURL := model.BaseURL
	if baseURL == "" {
		baseURL = cfg.BaseURL
	}
	if baseURL == "" {
		return ResolvedModel{}, fmt.Errorf("provider %q model %q: missing baseUrl", providerID, model.ID)
	}

	input := model.Input
	if len(input) == 0 {
		input = []string{"text"}
	}

	contextWindow := model.ContextWindow
	if contextWindow == 0 {
		contextWindow = defaultContextWindow
	}

	maxTokens := model.MaxTokens
	if maxTokens == 0 {
		maxTokens = defaultMaxTokens
	}

	cost := Cost{}
	if model.Cost != nil {
		cost = *model.Cost
	}

	headers := mergeStringMaps(cfg.Headers, model.Headers)

	return ResolvedModel{
		ID:            model.ID,
		Name:          name,
		ProviderID:    providerID,
		ProviderName:  providerName,
		API:           api,
		BaseURL:       baseURL,
		Reasoning:     model.Reasoning,
		Input:         input,
		ContextWindow: contextWindow,
		MaxTokens:     maxTokens,
		Cost:          cost,
		Headers:       headers,
	}, nil
}

func mergeStringMaps(base, override map[string]string) map[string]string {
	if len(base) == 0 && len(override) == 0 {
		return nil
	}
	out := make(map[string]string, len(base)+len(override))
	for key, value := range base {
		out[key] = value
	}
	for key, value := range override {
		out[key] = value
	}
	return out
}
