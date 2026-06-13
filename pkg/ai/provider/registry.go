package provider

import (
	"os"
)

const elphModelEnv = "ELPH_MODEL"

// Config holds the resolved default provider and display metadata.
type Config struct {
	Provider Provider
	Model    string
	ID       string
}

// ResolveFromEnv picks the first configured provider from environment variables.
// Priority: Anthropic → OpenAI → DeepSeek. Returns a zero Config when no API key is set.
func ResolveFromEnv() Config {
	if model := os.Getenv(elphModelEnv); model != "" {
		if cfg, ok := resolveWithModel(model); ok {
			return cfg
		}
	}

	if p, err := NewAnthropicFromEnv(); err == nil {
		return Config{Provider: p, Model: p.Model, ID: p.ID()}
	}
	if p, err := NewOpenAIFromEnv(); err == nil {
		return Config{Provider: p, Model: p.DefaultModel, ID: p.ID()}
	}
	if p, err := newDeepSeekFromEnv(); err == nil {
		return Config{Provider: p, Model: p.DefaultModel, ID: p.ID()}
	}
	return Config{}
}

func resolveWithModel(model string) (Config, bool) {
	if apiKey := os.Getenv(anthropicAPIKeyEnv); apiKey != "" {
		p := NewAnthropic(apiKey, model)
		return Config{Provider: p, Model: model, ID: p.ID()}, true
	}
	if apiKey := os.Getenv(openAIAPIKeyEnv); apiKey != "" {
		p := NewOpenAICompatible(IDOpenAI, apiKey, os.Getenv(openAIBaseURLEnv), model)
		return Config{Provider: p, Model: model, ID: p.ID()}, true
	}
	if apiKey := os.Getenv(deepSeekAPIKeyEnv); apiKey != "" {
		p := NewOpenAICompatible(IDDeepSeek, apiKey, deepSeekBaseURL, model)
		return Config{Provider: p, Model: model, ID: p.ID()}, true
	}
	return Config{}, false
}