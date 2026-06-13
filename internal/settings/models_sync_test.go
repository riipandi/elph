package settings

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestRunModelsSyncIfDueSkipsWithinInterval(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	now := time.Date(2026, 6, 13, 12, 0, 0, 0, time.UTC)
	require.NoError(t, MarkModelsSynced(now))

	result, ran, err := RunModelsSyncIfDue(now.Add(2 * time.Hour))
	require.NoError(t, err)
	require.False(t, ran)
	require.Empty(t, result.Updated)
}

func TestRunModelsSyncRecordsTimestamp(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	providersDir := filepath.Join(home, ".elph", "providers")
	require.NoError(t, os.MkdirAll(providersDir, 0o755))
	t.Setenv("ELPH_PROVIDERS_DIR", providersDir)

	catalog := provider.ModelsDevCatalog{
		Providers: map[string]provider.ModelsDevProvider{
			"openai": {
				ID: "openai",
				Models: map[string]provider.ModelsDevModel{
					"gpt-4o": {
						ID:    "gpt-4o",
						Name:  "GPT-4o",
						Limit: provider.ModelsDevLimit{Context: 128000, Output: 16384},
					},
				},
			},
		},
	}

	cfg := provider.FileConfig{
		BaseURL: "https://api.openai.com/v1",
		API:     provider.APIOpenAICompletions,
		APIKey:  "test",
		Models:  []provider.ModelConfig{{ID: "gpt-4o", Name: "Old"}},
	}
	raw, err := json.MarshalIndent(cfg, "", "  ")
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(providersDir, "openai.json"), append(raw, '\n'), 0o644))

	_, err = provider.UpdateModelsFromModelsDev(provider.UpdateModelsOptions{
		Dir: providersDir,
		Data: provider.ModelsDevData{
			Catalog: catalog,
			Models:  map[string]provider.ModelsDevModel{},
		},
	})
	require.NoError(t, err)
	require.NoError(t, MarkModelsSynced(time.Date(2026, 6, 13, 15, 30, 0, 0, time.UTC)))

	settings, err := Load()
	require.NoError(t, err)
	require.Equal(t, "2026-06-13T15:30:00Z", settings.Models.LastSync)
}
