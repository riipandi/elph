package settings

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/riipandi/elph/internal/theme"
)

const (
	defaultElphHomeDir        = ".elph"
	settingsFileName          = "settings.json"
	DefaultModelsSyncInterval = 24 * time.Hour
)

// Settings is persisted at ~/.elph/settings.json.
type Settings struct {
	Models             ModelsSettings  `json:"models"`
	Theme              string          `json:"theme,omitempty"`
	ShowThinking       *bool           `json:"showThinking,omitempty"`
	AutoExpandThinking *bool           `json:"autoExpandThinking,omitempty"`
	Session            SessionSettings `json:"session,omitempty"`
}

// ModelsSettings controls periodic models.dev metadata sync.
type ModelsSettings struct {
	LastSync     string `json:"lastSync,omitempty"`
	SyncInterval string `json:"syncInterval,omitempty"`
}

// Path returns ~/.elph/settings.json.
func Path() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, defaultElphHomeDir, settingsFileName), nil
}

// Load reads settings from disk. Missing files return defaults.
func Load() (Settings, error) {
	path, err := Path()
	if err != nil {
		return Settings{}, err
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return defaultSettings(), nil
		}
		return Settings{}, fmt.Errorf("read settings %q: %w", path, err)
	}

	var cfg Settings
	if err := json.Unmarshal(raw, &cfg); err != nil {
		return Settings{}, fmt.Errorf("decode settings %q: %w", path, err)
	}
	return cfg.withDefaults(), nil
}

// Save writes settings to ~/.elph/settings.json.
func Save(cfg Settings) error {
	path, err := Path()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("create settings dir: %w", err)
	}

	cfg = cfg.withDefaults()
	payload, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return fmt.Errorf("encode settings: %w", err)
	}
	payload = append(payload, '\n')

	if err := os.WriteFile(path, payload, 0o644); err != nil {
		return fmt.Errorf("write settings %q: %w", path, err)
	}
	return nil
}

func defaultSettings() Settings {
	showThinking := true
	return Settings{
		Models: ModelsSettings{
			SyncInterval: "24h",
		},
		Theme:        string(theme.Auto),
		ShowThinking: &showThinking,
	}
}

func (s Settings) withDefaults() Settings {
	s.Models = s.Models.withDefaults()
	if s.ShowThinking == nil {
		v := true
		s.ShowThinking = &v
	}
	if s.AutoExpandThinking == nil {
		v := false
		s.AutoExpandThinking = &v
	}
	return s
}

// ShowThinkingEnabled reports whether reasoning output is streamed in the UI.
func (s Settings) ShowThinkingEnabled() bool {
	return *s.withDefaults().ShowThinking
}

// AutoExpandThinkingEnabled reports whether thinking blocks start expanded.
func (s Settings) AutoExpandThinkingEnabled() bool {
	return *s.withDefaults().AutoExpandThinking
}

func (m ModelsSettings) withDefaults() ModelsSettings {
	if m.SyncInterval == "" {
		m.SyncInterval = "24h"
	}
	return m
}

// SyncIntervalDuration parses models.syncInterval (default 24h).
func (m ModelsSettings) SyncIntervalDuration() time.Duration {
	if m.SyncInterval == "" {
		return DefaultModelsSyncInterval
	}
	d, err := time.ParseDuration(m.SyncInterval)
	if err != nil || d <= 0 {
		return DefaultModelsSyncInterval
	}
	return d
}

// LastSyncTime returns the parsed last sync timestamp.
func (m ModelsSettings) LastSyncTime() (time.Time, bool) {
	if m.LastSync == "" {
		return time.Time{}, false
	}
	t, err := time.Parse(time.RFC3339, m.LastSync)
	if err != nil {
		return time.Time{}, false
	}
	return t, true
}

// SyncDue reports whether a models.dev sync should run at now.
func (s Settings) SyncDue(now time.Time) bool {
	last, ok := s.Models.LastSyncTime()
	if !ok {
		return true
	}
	return !now.Before(last.Add(s.Models.SyncIntervalDuration()))
}

// MarkModelsSynced records a successful models.dev fetch/sync at now.
func MarkModelsSynced(now time.Time) error {
	cfg, err := Load()
	if err != nil {
		return err
	}
	cfg.Models.LastSync = now.UTC().Format(time.RFC3339)
	return Save(cfg)
}

// IsNotExist reports whether err is a missing settings file.
func IsNotExist(err error) bool {
	return errors.Is(err, os.ErrNotExist)
}
