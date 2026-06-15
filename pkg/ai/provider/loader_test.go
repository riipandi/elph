package provider

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func writeProviderFile(t *testing.T, dir, name, body string) {
	t.Helper()
	require.NoError(t, os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644))
}

func TestLoadCatalogAppliesDeepSeekDeveloperRoleCompat(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "deepseek.json", `{
		"name": "DeepSeek",
		"baseUrl": "https://api.deepseek.com",
		"api": "openai-completions",
		"apiKey": "env.DEEPSEEK_API_KEY",
		"authHeader": true,
		"models": [
			{
				"id": "deepseek-reasoner",
				"name": "DeepSeek Reasoner",
				"reasoning": true
			}
		]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)

	model := catalog.Providers[0].Models[0]
	require.NotNil(t, model.Compat.SupportsDeveloperRole)
	require.False(t, *model.Compat.SupportsDeveloperRole)
}

func TestLoadCatalogAppliesOpenCodeGatewayThinkingCompat(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode-go.json", `{
		"name": "OpenCode Go",
		"baseUrl": "https://opencode.ai/zen/go/v1",
		"api": "openai-completions",
		"apiKey": "env.OPENCODE_API_KEY",
		"authHeader": true,
		"models": [
			{
				"id": "mimo-v2.5",
				"name": "MiMo V2.5",
				"reasoning": true
			}
		]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)

	model := catalog.Providers[0].Models[0]
	require.Equal(t, string(ThinkingFormatQwen), model.Compat.ThinkingFormat)
	require.NotNil(t, model.Compat.SupportsReasoningEffort)
	require.False(t, *model.Compat.SupportsReasoningEffort)
}

func TestLoadCatalogFromDir(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"name": "OpenCode",
		"baseUrl": "https://api.opencode.ai/v1",
		"api": "openai-completions",
		"apiKey": "$OPENCODE_API_KEY",
		"authHeader": true,
		"headers": {
			"X-Custom": "value"
		},
		"models": [
			{
				"id": "opencode-v1",
				"name": "OpenCode V1"
			}
		]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Empty(t, catalog.Errors)

	provider := catalog.Providers[0]
	require.Equal(t, "opencode", provider.ID)
	require.Equal(t, "OpenCode", provider.Config.Name)
	require.Len(t, provider.Models, 1)

	model := provider.Models[0]
	require.Equal(t, "opencode-v1", model.ID)
	require.Equal(t, "OpenCode V1", model.Name)
	require.Equal(t, APIOpenAICompletions, model.API)
	require.Equal(t, "https://api.opencode.ai/v1", model.BaseURL)
	require.Equal(t, defaultContextWindow, model.ContextWindow)
	require.Equal(t, defaultMaxTokens, model.MaxTokens)
	require.Equal(t, defaultTemperature, model.Temperature)
	require.Equal(t, defaultTopP, model.TopP)
	require.Equal(t, map[string]string{"X-Custom": "value"}, model.Headers)
}

func TestLoadCatalogModelTemperatureOverride(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "openai.json", `{
		"baseUrl": "https://api.openai.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [
			{"id": "default-temp"},
			{"id": "custom-temp", "temperature": 0.2}
		]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Len(t, catalog.Providers[0].Models, 2)
	require.Equal(t, defaultTemperature, catalog.Providers[0].Models[0].Temperature)
	require.Equal(t, 0.2, catalog.Providers[0].Models[1].Temperature)
}

func TestLoadCatalogModelTopPOverride(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "openai.json", `{
		"baseUrl": "https://api.openai.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [
			{"id": "default-top-p"},
			{"id": "custom-top-p", "topP": 0.95}
		]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Len(t, catalog.Providers[0].Models, 2)
	require.Equal(t, defaultTopP, catalog.Providers[0].Models[0].TopP)
	require.Equal(t, 0.95, catalog.Providers[0].Models[1].TopP)
}

func TestLoadCatalogProviderJSONC(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "demo.jsonc", `{
		// provider with comments
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "m1", "name": "Model 1"},],
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Equal(t, "demo", catalog.Providers[0].ID)
	require.Equal(t, "m1", catalog.Providers[0].Models[0].ID)
}

func TestLoadCatalogPrefersJSONOverJSONC(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "demo.jsonc", `{
		"baseUrl": "https://jsonc.example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "from-jsonc"}]
	}`)
	writeProviderFile(t, dir, "demo.json", `{
		"baseUrl": "https://json.example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "from-json"}]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Equal(t, "from-json", catalog.Providers[0].Models[0].ID)
}

func TestLoadCatalogSkipsInvalidFiles(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "broken.json", `{invalid`)
	writeProviderFile(t, dir, "valid.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "m1"}]
	}`)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 1)
	require.Len(t, catalog.Errors, 1)
	require.Equal(t, "valid", catalog.Providers[0].ID)
}

func TestResolveCatalogWithEnv(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"baseUrl": "https://api.opencode.ai/v1",
		"api": "openai-completions",
		"apiKey": "$OPENCODE_API_KEY",
		"authHeader": true,
		"models": [
			{"id": "model-a", "name": "Model A"},
			{"id": "model-b", "name": "Model B"}
		]
	}`)
	t.Setenv("ELPH_PROVIDERS_DIR", dir)
	t.Setenv("OPENCODE_API_KEY", "secret")
	t.Setenv("ELPH_PROVIDER", "opencode")
	t.Setenv("ELPH_MODEL", "model-b")

	cfg := Resolve()
	require.NotNil(t, cfg.Provider)
	require.Equal(t, "opencode", cfg.ProviderID)
	require.Equal(t, "model-b", cfg.ModelID)
	require.Equal(t, "Model B", cfg.ModelName)
}
