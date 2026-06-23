package settings

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestEnsureCreatesSettingsJSONWhenMissing(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, Ensure())

	path := filepath.Join(home, ".elph", "settings.json")
	raw, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Contains(t, string(raw), `"syncInterval": "24h"`)
	require.Contains(t, string(raw), `"theme": "auto"`)
	require.Contains(t, string(raw), `"maxRetries": 2`)
	require.Contains(t, string(raw), `"defaultTimeout": "2m0s"`)

	cfg, err := Load()
	require.NoError(t, err)
	require.True(t, cfg.ShowThinkingEnabled())
}

func TestEnsureSkipsExistingSettingsJSON(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph")
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "settings.json"), []byte(`{"theme":"dark"}`), 0o644))

	require.NoError(t, Ensure())

	raw, err := os.ReadFile(filepath.Join(dir, "settings.json"))
	require.NoError(t, err)
	require.NotContains(t, string(raw), `"syncInterval"`)
}

func TestEnsureSkipsSettingsJSONC(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph")
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "settings.jsonc"), []byte(`{"theme":"dark"}`), 0o644))

	require.NoError(t, Ensure())

	_, err := os.Stat(filepath.Join(dir, "settings.json"))
	require.True(t, os.IsNotExist(err))
}

func TestLoadMissingReturnsDefaults(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "24h", cfg.SyncInterval)

	require.True(t, cfg.ShowThinkingEnabled())
	require.True(t, cfg.AutoExpandThinkingEnabled())
	require.False(t, cfg.UseRawPasteEnabled())
	require.True(t, cfg.StickyScrollEnabled())
	require.Equal(t, "auto", cfg.Theme)
	require.Equal(t, ResponseLanguageInherit, cfg.ResponseLanguage())
	require.Equal(t, DefaultProviderMaxRetries, cfg.ProviderMaxRetries())
	require.Equal(t, DefaultProviderTimeout, cfg.ProviderDefaultTimeout())
}

func TestAutoExpandThinkingDefaultsTrueAndCanDisable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.True(t, cfg.AutoExpandThinkingEnabled())

	disabled := false
	require.NoError(t, Save(Settings{
		SyncInterval:       cfg.SyncInterval,
		ShowThinking:       cfg.ShowThinking,
		AutoExpandThinking: &disabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.False(t, cfg.AutoExpandThinkingEnabled())
}

func TestUseRawPasteDefaultsFalseAndCanEnable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.False(t, cfg.UseRawPasteEnabled())

	enabled := true
	require.NoError(t, Save(Settings{
		SyncInterval: cfg.SyncInterval,
		ShowThinking: cfg.ShowThinking,
		UseRawPaste:  &enabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.True(t, cfg.UseRawPasteEnabled())
}

func TestPreferedResponseLanguageDefaultsInheritAndCanOverride(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, ResponseLanguageInherit, cfg.ResponseLanguage())

	require.NoError(t, Save(Settings{
		SyncInterval:             cfg.SyncInterval,
		ShowThinking:             cfg.ShowThinking,
		PreferedResponseLanguage: "Indonesian",
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.Equal(t, "Indonesian", cfg.ResponseLanguage())
}

func TestStickyScrollDefaultsTrueAndCanDisable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.True(t, cfg.StickyScrollEnabled())

	disabled := false
	require.NoError(t, Save(Settings{
		SyncInterval: cfg.SyncInterval,
		ShowThinking: cfg.ShowThinking,
		StickyScroll: &disabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.False(t, cfg.StickyScrollEnabled())
}

func TestShowThinkingDefaultsTrueAndCanDisable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.True(t, cfg.ShowThinkingEnabled())

	disabled := false
	require.NoError(t, Save(Settings{
		SyncInterval: cfg.SyncInterval,
		ShowThinking: &disabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.False(t, cfg.ShowThinkingEnabled())
}

func TestLoadLegacyNestedSyncInterval(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph")
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "settings.json"), []byte(`{
		"models": { "syncInterval": "6h" }
	}`), 0o644))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "6h", cfg.SyncInterval)

	require.NoError(t, Save(cfg))
	raw, err := os.ReadFile(filepath.Join(dir, "settings.json"))
	require.NoError(t, err)
	require.Contains(t, string(raw), `"syncInterval": "6h"`)
	require.NotContains(t, string(raw), `"models"`)
}

func TestSaveAndLoadRoundTrip(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, Save(Settings{
		SyncInterval: "12h",
	}))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "12h", cfg.SyncInterval)
	require.Equal(t, 12*time.Hour, cfg.SyncIntervalDuration())
}

func TestSyncDue(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	last := time.Date(2026, 6, 13, 0, 0, 0, 0, time.UTC)
	require.NoError(t, MarkProvidersSynced(last))

	cfg := Settings{
		SyncInterval: "24h",
	}

	require.False(t, cfg.SyncDue(last.Add(23*time.Hour)))
	require.True(t, cfg.SyncDue(last.Add(24*time.Hour)))
	require.True(t, Settings{}.SyncDue(time.Now()))
}

func TestMarkModelsSyncedWritesVersionFile(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	now := time.Date(2026, 6, 13, 15, 30, 0, 0, time.UTC)
	require.NoError(t, MarkModelsSynced(now))

	path, err := VersionPath()
	require.NoError(t, err)
	raw, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Contains(t, string(raw), `"lastSyncProviders": "2026-06-13T15:30:00Z"`)
}

func TestLoadSettingsJSONC(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph")
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "settings.jsonc"), []byte(`{
		// UI theme
		"theme": "dark",
	}`), 0o644))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "dark", cfg.Theme)

	path, err := Path()
	require.NoError(t, err)
	require.Equal(t, filepath.Join(dir, "settings.jsonc"), path)
}

func TestLoadSettingsJSONWithComments(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph")
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "settings.json"), []byte(`{
		/* block comment */
		"theme": "light",
	}`), 0o644))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "light", cfg.Theme)
}

func TestPathUsesElphHome(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	path, err := Path()
	require.NoError(t, err)
	require.Equal(t, filepath.Join(home, ".elph", "settings.json"), path)
}
