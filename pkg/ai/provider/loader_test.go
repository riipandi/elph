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
