package provider

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestBootstrapProvidersCreatesStarterFiles(t *testing.T) {
	dir := t.TempDir()

	result, err := BootstrapProviders(dir, false)
	require.NoError(t, err)
	require.Equal(t, dir, result.Dir)
	require.Equal(t, []string{"openai.json", "anthropic.json", "opencode.json", "opencode-go.json"}, result.Created)
	require.Empty(t, result.Skipped)

	for _, name := range result.Created {
		raw, err := os.ReadFile(filepath.Join(dir, name))
		require.NoError(t, err)

		var cfg FileConfig
		require.NoError(t, json.Unmarshal(raw, &cfg))
		require.NotEmpty(t, cfg.BaseURL)
		require.NotEmpty(t, cfg.API)
		require.NotEmpty(t, cfg.Models)
		for _, model := range cfg.Models {
			require.Greater(t, model.ContextWindow, 0, "%s missing contextWindow", name)
			require.Greater(t, model.MaxTokens, 0, "%s missing maxTokens", name)
		}
		if name == "openai.json" {
			require.Contains(t, string(raw), `"input": 2.50`)
			require.Contains(t, string(raw), `"output": 10.00`)
			require.Contains(t, string(raw), `"cacheRead": 0.075`)
		}
	}
}

func TestBootstrapProvidersSkipsExistingFiles(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "openai.json"), []byte(`{"custom":true}`), 0o644))

	result, err := BootstrapProviders(dir, false)
	require.NoError(t, err)
	require.Equal(t, []string{"anthropic.json", "opencode.json", "opencode-go.json"}, result.Created)
	require.Equal(t, []string{"openai.json"}, result.Skipped)

	raw, err := os.ReadFile(filepath.Join(dir, "openai.json"))
	require.NoError(t, err)
	require.Contains(t, string(raw), `"custom":true`)
}

func TestBootstrapProvidersForceOverwrites(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "openai.json"), []byte(`{"custom":true}`), 0o644))

	result, err := BootstrapProviders(dir, true)
	require.NoError(t, err)
	require.Equal(t, []string{"openai.json", "anthropic.json", "opencode.json", "opencode-go.json"}, result.Created)
	require.Empty(t, result.Skipped)

	raw, err := os.ReadFile(filepath.Join(dir, "openai.json"))
	require.NoError(t, err)
	require.Contains(t, string(raw), `"name": "OpenAI"`)
}
