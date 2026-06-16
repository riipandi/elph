package settings

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/theme"
)

const (
	defaultElphHomeDir        = ".elph"
	settingsFileName          = "settings.json"
	settingsJSONCFileName     = "settings.jsonc"
	DefaultModelsSyncInterval = 24 * time.Hour
	// ResponseLanguageInherit matches the user's message language automatically.
	ResponseLanguageInherit = "inherit"
)

// Settings is persisted at ~/.elph/settings.json.
// Settings is persisted at ~/.elph/settings.json.
type Settings struct {
	SyncInterval             string            `json:"syncInterval,omitempty"`
	Models                   *ModelsSettings   `json:"models,omitempty"`
	Theme                    string            `json:"theme,omitempty"`
	ShowThinking             *bool             `json:"showThinking,omitempty"`
	AutoExpandThinking       *bool             `json:"autoExpandThinking,omitempty"`
	UseRawPaste              *bool             `json:"useRawPaste,omitempty"`
	StickyScroll             *bool             `json:"stickyScroll,omitempty"`
	PreferedResponseLanguage string            `json:"preferedResponseLanguage,omitempty"`
	ThinkingBudgets          map[string]int    `json:"thinkingBudgets,omitempty"`
	Provider                 *ProviderSettings `json:"provider,omitempty"`
	Session                  SessionSettings   `json:"session,omitempty"`
	MaxToolIterations        *int              `json:"maxToolIterations,omitempty"`
	AutoCompactContext       *bool             `json:"autoCompactContext,omitempty"`
	AutoCompactLimit         *int              `json:"autoCompactLimit,omitempty"`
	CompactMinMessages       *int              `json:"compactMinMessages,omitempty"`  // Minimum messages before auto-compact
	CompactMinBytes          *int              `json:"compactMinBytes,omitempty"`     // Minimum bytes before auto-compact
	CompactContextUsage      *int              `json:"compactContextUsage,omitempty"` // Context usage % threshold (0-100)
	FooterTokenDisplay       string            `json:"footerTokenDisplay,omitempty"`
}

// ModelsSettings holds legacy settings migrated on load.
type ModelsSettings struct {
	// SyncInterval is legacy; promoted to Settings.SyncInterval on load.
	SyncInterval string `json:"syncInterval,omitempty"`
	// LastSync is legacy; migrated to version.json on load.
	LastSync string `json:"lastSync,omitempty"`
}

func (m ModelsSettings) legacyLastSync() string {
	return strings.TrimSpace(m.LastSync)
}

// Path returns the active home settings file path (~/.elph/settings.json or settings.jsonc).
func Path() (string, error) {
	dir, err := homeSettingsDir()
	if err != nil {
		return "", err
	}
	path, ok := activeSettingsPath(dir)
	if ok {
		return path, nil
	}
	return filepath.Join(dir, settingsFileName), nil
}

func settingsFileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// Ensure creates ~/.elph/settings.json with defaults when no settings file exists.
// Existing settings.json or settings.jsonc files are left unchanged.
func Ensure() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	dir := filepath.Join(home, defaultElphHomeDir)
	jsonPath := filepath.Join(dir, settingsFileName)
	jsoncPath := filepath.Join(dir, settingsJSONCFileName)
	if settingsFileExists(jsonPath) || settingsFileExists(jsoncPath) {
		return nil
	}
	return Save(defaultSettings())
}

// Load reads merged settings from ~/.elph and the current working directory.
func Load() (Settings, error) {
	wd, err := os.Getwd()
	if err != nil {
		wd = ""
	}
	return LoadFor(wd)
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
		SyncInterval:             "24h",
		Theme:                    string(theme.Auto),
		ShowThinking:             &showThinking,
		PreferedResponseLanguage: ResponseLanguageInherit,
		Provider:                 defaultProviderSettings(),
	}
}

func (s Settings) withDefaults() Settings {
	if strings.TrimSpace(s.SyncInterval) == "" && s.Models != nil && strings.TrimSpace(s.Models.SyncInterval) != "" {
		s.SyncInterval = strings.TrimSpace(s.Models.SyncInterval)
	}
	if s.Models != nil {
		s.Models.SyncInterval = ""
		if s.Models.LastSync == "" {
			s.Models = nil
		}
	}
	if strings.TrimSpace(s.SyncInterval) == "" {
		s.SyncInterval = "24h"
	}
	if s.ShowThinking == nil {
		v := true
		s.ShowThinking = &v
	}
	if s.AutoExpandThinking == nil {
		v := false
		s.AutoExpandThinking = &v
	}
	if s.UseRawPaste == nil {
		v := false
		s.UseRawPaste = &v
	}
	if s.StickyScroll == nil {
		v := true
		s.StickyScroll = &v
	}
	if strings.TrimSpace(s.PreferedResponseLanguage) == "" {
		s.PreferedResponseLanguage = ResponseLanguageInherit
	}
	if s.Provider == nil {
		s.Provider = defaultProviderSettings()
	} else {
		if s.Provider.MaxRetries == nil {
			maxRetries := DefaultProviderMaxRetries
			s.Provider.MaxRetries = &maxRetries
		}
		if strings.TrimSpace(s.Provider.DefaultTimeout) == "" {
			s.Provider.DefaultTimeout = DefaultProviderTimeout.String()
		}
	}
	if s.MaxToolIterations == nil {
		v := 0 // 0 = use DefaultMaxToolIterations in loop
		s.MaxToolIterations = &v
	}
	if s.AutoCompactContext == nil {
		v := true
		s.AutoCompactContext = &v
	}
	if s.AutoCompactLimit == nil {
		v := 80 // default 80%
		s.AutoCompactLimit = &v
	}
	if s.FooterTokenDisplay == "" {
		s.FooterTokenDisplay = string(FooterTokenBoth)
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

// UseRawPasteEnabled reports whether pasted text is inserted verbatim in the input.
// When false (default), long pastes collapse to a [Pasted: N lines] placeholder.
func (s Settings) UseRawPasteEnabled() bool {
	return *s.withDefaults().UseRawPaste
}

// StickyScrollEnabled reports whether user prompts pin to the top while
// scrolling through assistant replies.
func (s Settings) StickyScrollEnabled() bool {
	return *s.withDefaults().StickyScroll
}

// ResponseLanguage returns the default language for assistant replies.
func (s Settings) ResponseLanguage() string {
	return s.withDefaults().PreferedResponseLanguage
}

// ToolRoundsLimit returns the configured max tool rounds (0 = use default).
func (s Settings) ToolRoundsLimit() int {
	cfg := s.withDefaults()
	if cfg.MaxToolIterations == nil {
		return 0
	}
	return *cfg.MaxToolIterations
}

// AutoCompactContextEnabled reports whether the agent automatically compacts
// conversation history and retries when the provider reports context-limit errors.
func (s Settings) AutoCompactContextEnabled() bool {
	cfg := s.withDefaults()
	return cfg.AutoCompactContext != nil && *cfg.AutoCompactContext
}

// CompactLimit returns the compaction target percentage (1-100, default 80).
func (s Settings) CompactLimit() int {
	cfg := s.withDefaults()
	if cfg.AutoCompactLimit == nil {
		return 80
	}
	limit := *cfg.AutoCompactLimit
	if limit < 10 {
		return 10
	}
	if limit > 100 {
		return 100
	}
	return limit
}

// GetCompactMinMessages returns the minimum messages before auto-compact, defaulting to 10.
func (s Settings) GetCompactMinMessages() int {
	cfg := s.withDefaults()
	if cfg.CompactMinMessages == nil {
		return 10
	}
	min := *cfg.CompactMinMessages
	if min < 4 {
		return 4
	}
	if min > 50 {
		return 50
	}
	return min
}

// GetCompactMinBytes returns the minimum bytes before auto-compact, defaulting to 64KB.
func (s Settings) GetCompactMinBytes() int {
	cfg := s.withDefaults()
	if cfg.CompactMinBytes == nil {
		return 64 * 1024 // 64KB
	}
	min := *cfg.CompactMinBytes
	if min < 1024 {
		return 1024 // 1KB minimum
	}
	return min
}

// GetCompactContextUsage returns the context usage % threshold for auto-compact, defaulting to 70.
func (s Settings) GetCompactContextUsage() int {
	cfg := s.withDefaults()
	if cfg.CompactContextUsage == nil {
		return 70
	}
	usage := *cfg.CompactContextUsage
	if usage < 50 {
		return 50
	}
	if usage > 95 {
		return 95
	}
	return usage
}

// FooterTokenDisplayMode returns the footer token display mode, defaulting to "both".
func (s Settings) FooterTokenDisplayMode() FooterTokenDisplay {
	return ParseFooterTokenDisplay(s.withDefaults().FooterTokenDisplay)
}

// ThinkingBudgetOverrides returns custom token budgets per thinking level.
func (s Settings) ThinkingBudgetOverrides() map[string]int {
	if len(s.ThinkingBudgets) == 0 {
		return nil
	}
	return s.ThinkingBudgets
}

// SyncIntervalDuration parses syncInterval (default 24h).
func (s Settings) SyncIntervalDuration() time.Duration {
	interval := strings.TrimSpace(s.withDefaults().SyncInterval)
	if interval == "" {
		return DefaultModelsSyncInterval
	}
	d, err := time.ParseDuration(interval)
	if err != nil || d <= 0 {
		return DefaultModelsSyncInterval
	}
	return d
}

// SyncDue reports whether a models.dev sync should run at now.
func (s Settings) SyncDue(now time.Time) bool {
	v, err := LoadVersion()
	if err != nil {
		return true
	}
	last, ok := v.LastSyncProvidersTime()
	if !ok {
		return true
	}
	return !now.Before(last.Add(s.SyncIntervalDuration()))
}

// MarkModelsSynced records a successful models.dev fetch/sync at now.
func MarkModelsSynced(now time.Time) error {
	return MarkProvidersSynced(now)
}

// IsNotExist reports whether err is a missing settings file.
func IsNotExist(err error) bool {
	return errors.Is(err, os.ErrNotExist)
}
