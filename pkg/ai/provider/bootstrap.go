package provider

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/riipandi/elph/pkg/jsoncfg"
)

// BootstrapResult reports which primary provider files were created or skipped.
type BootstrapResult struct {
	Dir        string
	Created    []string
	Skipped    []string
	Backfilled []string
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
						ThinkingLevelMap: map[string]json.RawMessage{
							"off":   json.RawMessage(`null`),
							"xhigh": json.RawMessage(`"high"`),
						},
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
						Compat: Compat{
							ForceAdaptiveThinking: true,
						},
						ThinkingLevelMap: map[string]json.RawMessage{
							"xhigh": json.RawMessage(`"max"`),
						},
					},
				},
			},
		},
		{
			ID: "opencode",
			Config: FileConfig{
				Name:       "OpenCode Zen",
				BaseURL:    OpenCodeZenBaseURL,
				API:        APIOpenAICompletions,
				APIKey:     "env.OPENCODE_API_KEY",
				AuthHeader: true,
				Compat: Compat{
					ThinkingFormat:          string(ThinkingFormatQwen),
					SupportsReasoningEffort: new(false),
				},
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
		{
			ID: "opencode-go",
			Config: FileConfig{
				Name:       "OpenCode Go",
				BaseURL:    OpenCodeGoBaseURL,
				API:        APIOpenAICompletions,
				APIKey:     "env.OPENCODE_API_KEY",
				AuthHeader: true,
				Compat: Compat{
					ThinkingFormat:          string(ThinkingFormatQwen),
					SupportsReasoningEffort: new(false),
				},
				Models: []ModelConfig{
					{
						ID:            "kimi-k2.5",
						Name:          "Kimi K2.5",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 262144,
						MaxTokens:     65536,
					},
					{
						ID:            "deepseek-v4-flash",
						Name:          "DeepSeek V4 Flash",
						ContextWindow: 1000000,
						MaxTokens:     384000,
					},
				},
			},
		},
		{
			ID: "deepseek",
			Config: FileConfig{
				Name:       "DeepSeek",
				BaseURL:    "https://api.deepseek.com",
				API:        APIOpenAICompletions,
				APIKey:     "env.DEEPSEEK_API_KEY",
				AuthHeader: true,
				Compat: Compat{
					SupportsDeveloperRole: new(false),
				},
				Models: []ModelConfig{
					{
						ID:            "deepseek-chat",
						Name:          "DeepSeek Chat",
						Input:         []string{"text"},
						ContextWindow: 1000000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.14, Output: 0.28, CacheRead: 0.0028, CacheWrite: 0},
					},
					{
						ID:            "deepseek-reasoner",
						Name:          "DeepSeek Reasoner",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 1000000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.14, Output: 0.28, CacheRead: 0.0028, CacheWrite: 0},
						Compat: Compat{
							ThinkingFormat: string(ThinkingFormatDeepSeek),
						},
					},
					{
						ID:            "deepseek-v4-flash",
						Name:          "DeepSeek V4 Flash",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 1000000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.14, Output: 0.28, CacheRead: 0.0028, CacheWrite: 0},
					},
					{
						ID:            "deepseek-v4-pro",
						Name:          "DeepSeek V4 Pro",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 1000000,
						MaxTokens:     8192,
						Cost:          &Cost{Input: 0.435, Output: 0.87, CacheRead: 0.003625, CacheWrite: 0},
						ThinkingLevelMap: map[string]json.RawMessage{
							"minimal": json.RawMessage(`null`),
							"low":     json.RawMessage(`null`),
							"medium":  json.RawMessage(`null`),
							"high":    json.RawMessage(`"high"`),
							"xhigh":   json.RawMessage(`"max"`),
						},
					},
				},
			},
		},
		{
			ID: "kimi",
			Config: FileConfig{
				Name:       "Kimi",
				BaseURL:    "https://api.moonshot.ai/v1",
				API:        APIOpenAICompletions,
				APIKey:     "env.MOONSHOT_API_KEY",
				AuthHeader: true,
				Compat: Compat{
					SupportsDeveloperRole: new(false),
				},
				Models: []ModelConfig{
					{
						ID:            "kimi-k2.5",
						Name:          "Kimi K2.5",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 262144,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 0.6, Output: 3, CacheRead: 0.1, CacheWrite: 0},
					},
					{
						ID:            "kimi-k2.6",
						Name:          "Kimi K2.6",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 262144,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 0.95, Output: 4, CacheRead: 0.16, CacheWrite: 0},
					},
					{
						ID:            "kimi-k2-thinking",
						Name:          "Kimi K2 Thinking",
						Reasoning:     true,
						Input:         []string{"text"},
						ContextWindow: 262144,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 0.6, Output: 2.5, CacheRead: 0.15, CacheWrite: 0},
					},
					{
						ID:            "kimi-k2.7-code",
						Name:          "Kimi K2.7 Code",
						Reasoning:     true,
						Input:         []string{"text", "image"},
						ContextWindow: 262144,
						MaxTokens:     16384,
						Cost:          &Cost{Input: 0.95, Output: 4, CacheRead: 0.19, CacheWrite: 0},
					},
				},
			},
		},
	}
}

// BootstrapOptions configures provider bootstrap.
type BootstrapOptions struct {
	Dir      string
	Force    bool
	Reporter ProviderProgressReporter
}

// BootstrapProviders writes starter provider JSON files into dir.
// Existing files are skipped unless force is true.
func BootstrapProviders(dir string, force bool) (BootstrapResult, error) {
	return BootstrapProvidersWithOptions(BootstrapOptions{Dir: dir, Force: force})
}

// BootstrapProvidersWithOptions writes or backfills starter provider files.
func BootstrapProvidersWithOptions(opts BootstrapOptions) (BootstrapResult, error) {
	dir := opts.Dir
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

	templates := primaryTemplates()
	total := len(templates)
	result := BootstrapResult{Dir: dir}
	for i, tmpl := range templates {
		filename := tmpl.ID + ".json"
		path := filepath.Join(dir, filename)
		label := strings.TrimSpace(tmpl.Config.Name)
		if label == "" {
			label = tmpl.ID
		}

		reportProviderProgress(opts.Reporter, ProviderProgressEvent{
			Phase:      ProviderProgressConnect,
			ProviderID: tmpl.ID,
			Label:      label,
			Index:      i + 1,
			Total:      total,
			Action:     ProviderProgressWorking,
		})

		if _, err := os.Stat(path); err == nil && !opts.Force {
			raw, readErr := os.ReadFile(path)
			if readErr != nil {
				return result, fmt.Errorf("read %q: %w", path, readErr)
			}
			var cfg FileConfig
			if err := jsoncfg.Unmarshal(raw, &cfg); err != nil {
				return result, fmt.Errorf("decode %q: %w", path, err)
			}
			updated, changed := BackfillProviderThinking(tmpl.ID, cfg)
			if changed {
				payload, err := json.MarshalIndent(updated, "", "  ")
				if err != nil {
					return result, fmt.Errorf("encode %q: %w", filename, err)
				}
				payload = append(payload, '\n')
				if err := os.WriteFile(path, payload, 0o644); err != nil {
					return result, fmt.Errorf("write %q: %w", path, err)
				}
				result.Backfilled = append(result.Backfilled, filename)
				reportProviderProgress(opts.Reporter, ProviderProgressEvent{
					Phase:      ProviderProgressConnect,
					ProviderID: tmpl.ID,
					Label:      label,
					Index:      i + 1,
					Total:      total,
					Action:     ProviderProgressBackfill,
				})
			} else {
				result.Skipped = append(result.Skipped, filename)
				reportProviderProgress(opts.Reporter, ProviderProgressEvent{
					Phase:      ProviderProgressConnect,
					ProviderID: tmpl.ID,
					Label:      label,
					Index:      i + 1,
					Total:      total,
					Action:     ProviderProgressUnchanged,
				})
			}
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
		reportProviderProgress(opts.Reporter, ProviderProgressEvent{
			Phase:      ProviderProgressConnect,
			ProviderID: tmpl.ID,
			Label:      label,
			Index:      i + 1,
			Total:      total,
			Action:     ProviderProgressCreated,
		})
	}
	return result, nil
}
