package provider

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// BootstrapResult reports which primary provider files were created or skipped.
type BootstrapResult struct {
	Dir     string
	Created []string
	Skipped []string
}

type bootstrapTemplate struct {
	ID     string
	Config FileConfig
}

// PrimaryProviderTemplates returns the built-in starter provider definitions.
func PrimaryProviderTemplates() map[string]FileConfig {
	templates := primaryTemplates()
	out := make(map[string]FileConfig, len(templates))
	for _, tmpl := range templates {
		out[tmpl.ID] = tmpl.Config
	}
	return out
}

func primaryTemplates() []bootstrapTemplate {
	return []bootstrapTemplate{
		{
			ID: "openai",
			Config: FileConfig{
				Name:       "OpenAI",
				BaseURL:    "https://api.openai.com/v1",
				API:        APIOpenAICompletions,
				APIKey:     "env.OPENAI_API_KEY",
				AuthHeader: true,
				Models: []ModelConfig{
					{
						ID:            "gpt-4o",
						Name:          "GPT-4o",
						Input:         []string{"text", "image"},
						ContextWindow: 128000,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 2.5, Output: 10, CacheRead: 1.25, CacheWrite: 0},
					},
					{
						ID:            "gpt-4o-mini",
						Name:          "GPT-4o Mini",
						Input:         []string{"text", "image"},
						ContextWindow: 128000,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 0.15, Output: 0.6, CacheRead: 0.075, CacheWrite: 0},
					},
					{
						ID:            "o3-mini",
						Name:          "o3-mini",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 200000,
						MaxTokens:     100000,
						Cost:          &Cost{Input: 1.1, Output: 4.4, CacheRead: 0.55, CacheWrite: 0},
					},
				},
			},
		},
		{
			ID: "anthropic",
			Config: FileConfig{
				Name:    "Anthropic",
				BaseURL: "https://api.anthropic.com/v1",
				API:     APIAnthropicMessages,
				APIKey:  "env.ANTHROPIC_API_KEY",
				Models: []ModelConfig{
					{
						ID:            "claude-sonnet-4-20250514",
						Name:          "Claude Sonnet 4",
						Input:         []string{"text", "image"},
						ContextWindow: 200000,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 3, Output: 15, CacheRead: 0.3, CacheWrite: 3.75},
					},
					{
						ID:            "claude-3-5-haiku-20241022",
						Name:          "Claude Haiku 3.5",
						ContextWindow: 200000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.8, Output: 4, CacheRead: 0.08, CacheWrite: 1},
					},
					{
						ID:            "claude-opus-4-20250514",
						Name:          "Claude Opus 4",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 200000,
						MaxTokens:     32000,
						Cost:          &Cost{Input: 15, Output: 75, CacheRead: 1.5, CacheWrite: 18.75},
					},
				},
			},
		},
		{
			ID: "opencode",
			Config: FileConfig{
				Name:       "OpenCode Zen",
				BaseURL:    "https://opencode.ai/zen/v1",
				API:        APIOpenAICompletions,
				APIKey:     "env.OPENCODE_API_KEY",
				AuthHeader: true,
				Models: []ModelConfig{
					{
						ID:            "big-pickle",
						Name:          "Big Pickle",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 128000,
						MaxTokens:     16384,
						Cost:          &Cost{},
					},
					{
						ID:            "claude-sonnet-4-6",
						Name:          "Claude Sonnet 4.6",
						API:           APIAnthropicMessages,
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 200000,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 3, Output: 15, CacheRead: 0.3, CacheWrite: 3.75},
					},
					{
						ID:            "kimi-k2.5",
						Name:          "Kimi K2.5",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 262144,
						MaxTokens:     262144,
						Cost:          &Cost{Input: 0.6, Output: 3, CacheRead: 0.1, CacheWrite: 0},
					},
					{
						ID:            "deepseek-v4-flash",
						Name:          "DeepSeek V4 Flash",
						ContextWindow: 128000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.14, Output: 0.28, CacheRead: 0.028, CacheWrite: 0},
					},
				},
			},
		},
	}
}

// BootstrapProviders writes starter provider JSON files into dir.
// Existing files are skipped unless force is true.
func BootstrapProviders(dir string, force bool) (BootstrapResult, error) {
	if dir == "" {
		var err error
		dir, err = ProvidersDir()
		if err != nil {
			return BootstrapResult{}, err
		}
	}

	if err := os.MkdirAll(dir, 0o755); err != nil {
		return BootstrapResult{}, fmt.Errorf("create providers dir %q: %w", dir, err)
	}

	result := BootstrapResult{Dir: dir}
	for _, tmpl := range primaryTemplates() {
		filename := tmpl.ID + ".json"
		path := filepath.Join(dir, filename)

		if _, err := os.Stat(path); err == nil && !force {
			result.Skipped = append(result.Skipped, filename)
			continue
		} else if err != nil && !os.IsNotExist(err) {
			return result, fmt.Errorf("stat %q: %w", path, err)
		}

		payload, err := json.MarshalIndent(tmpl.Config, "", "  ")
		if err != nil {
			return result, fmt.Errorf("encode %q: %w", filename, err)
		}
		payload = append(payload, '\n')

		if err := os.WriteFile(path, payload, 0o644); err != nil {
			return result, fmt.Errorf("write %q: %w", path, err)
		}
		result.Created = append(result.Created, filename)
	}
	return result, nil
}
