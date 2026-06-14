package settings

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestLoadMissingReturnsDefaults(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "24h", cfg.Models.SyncInterval)
	require.Empty(t, cfg.Models.LastSync)
	require.True(t, cfg.ShowThinkingEnabled())
	require.False(t, cfg.AutoExpandThinkingEnabled())
	require.Equal(t, "auto", cfg.Theme)
}

func TestAutoExpandThinkingDefaultsFalseAndCanEnable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.False(t, cfg.AutoExpandThinkingEnabled())

	enabled := true
	require.NoError(t, Save(Settings{
		Models:             cfg.Models,
		ShowThinking:       cfg.ShowThinking,
		AutoExpandThinking: &enabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.True(t, cfg.AutoExpandThinkingEnabled())
}

func TestShowThinkingDefaultsTrueAndCanDisable(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.True(t, cfg.ShowThinkingEnabled())

	disabled := false
	require.NoError(t, Save(Settings{
		Models:       cfg.Models,
		ShowThinking: &disabled,
	}))

	cfg, err = Load()
	require.NoError(t, err)
	require.False(t, cfg.ShowThinkingEnabled())
}

func TestSaveAndLoadRoundTrip(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	ts := time.Date(2026, 6, 13, 10, 0, 0, 0, time.UTC)
	require.NoError(t, Save(Settings{
		Models: ModelsSettings{
			LastSync:     ts.Format(time.RFC3339),
			SyncInterval: "12h",
		},
	}))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, ts.Format(time.RFC3339), cfg.Models.LastSync)
	require.Equal(t, "12h", cfg.Models.SyncInterval)
	require.Equal(t, 12*time.Hour, cfg.Models.SyncIntervalDuration())
}

func TestSyncDue(t *testing.T) {
	last := time.Date(2026, 6, 13, 0, 0, 0, 0, time.UTC)
	cfg := Settings{
		Models: ModelsSettings{
			LastSync:     last.Format(time.RFC3339),
			SyncInterval: "24h",
		},
	}

	require.False(t, cfg.SyncDue(last.Add(23*time.Hour)))
	require.True(t, cfg.SyncDue(last.Add(24*time.Hour)))
	require.True(t, Settings{}.SyncDue(time.Now()))
}

func TestMarkModelsSyncedWritesFile(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	now := time.Date(2026, 6, 13, 15, 30, 0, 0, time.UTC)
	require.NoError(t, MarkModelsSynced(now))

	path, err := Path()
	require.NoError(t, err)
	raw, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Contains(t, string(raw), `"lastSync": "2026-06-13T15:30:00Z"`)
	require.Contains(t, string(raw), `"syncInterval"`)
}

func TestPathUsesElphHome(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	path, err := Path()
	require.NoError(t, err)
	require.Equal(t, filepath.Join(home, ".elph", "settings.json"), path)
}
