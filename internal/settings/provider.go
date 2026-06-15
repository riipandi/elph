package settings

import (
	"strings"
	"time"
)

const (
	// DefaultProviderMaxRetries is how many times to retry a retriable provider failure.
	DefaultProviderMaxRetries = 2
	// DefaultProviderTimeout is the default provider/SSE inactivity limit.
	DefaultProviderTimeout = 120 * time.Second
)

// ProviderSettings configures upstream provider retry and streaming timeouts.
type ProviderSettings struct {
	MaxRetries     *int   `json:"maxRetries,omitempty"`
	DefaultTimeout string `json:"defaultTimeout,omitempty"`
}

// ProviderMaxRetries returns configured retry count (0 = no retries).
func (s Settings) ProviderMaxRetries() int {
	cfg := s.withDefaults().Provider
	if cfg == nil || cfg.MaxRetries == nil {
		return DefaultProviderMaxRetries
	}
	if *cfg.MaxRetries < 0 {
		return 0
	}
	return *cfg.MaxRetries
}

// ProviderDefaultTimeout parses the configured provider inactivity timeout.
func (s Settings) ProviderDefaultTimeout() time.Duration {
	cfg := s.withDefaults().Provider
	raw := ""
	if cfg != nil {
		raw = strings.TrimSpace(cfg.DefaultTimeout)
	}
	if raw == "" {
		return DefaultProviderTimeout
	}
	d, err := time.ParseDuration(raw)
	if err != nil || d <= 0 {
		return DefaultProviderTimeout
	}
	return d
}

func defaultProviderSettings() *ProviderSettings {
	maxRetries := DefaultProviderMaxRetries
	return &ProviderSettings{
		MaxRetries:     &maxRetries,
		DefaultTimeout: DefaultProviderTimeout.String(),
	}
}

func mergeProviderSettings(base, overlay *ProviderSettings) *ProviderSettings {
	if overlay == nil {
		return base
	}
	if base == nil {
		base = defaultProviderSettings()
	}
	merged := *base
	if overlay.MaxRetries != nil {
		merged.MaxRetries = cloneInt(overlay.MaxRetries)
	}
	if strings.TrimSpace(overlay.DefaultTimeout) != "" {
		merged.DefaultTimeout = strings.TrimSpace(overlay.DefaultTimeout)
	}
	return &merged
}

func cloneInt(v *int) *int {
	if v == nil {
		return nil
	}
	n := *v
	return &n
}
