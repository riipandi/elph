package settings

import (
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestProviderSettingsDefaults(t *testing.T) {
	t.Parallel()

	cfg := defaultSettings().withDefaults()
	require.NotNil(t, cfg.Provider)
	require.Equal(t, DefaultProviderMaxRetries, cfg.ProviderMaxRetries())
	require.Equal(t, DefaultProviderTimeout, cfg.ProviderDefaultTimeout())
}

func TestProviderSettingsOverride(t *testing.T) {
	t.Parallel()

	maxRetries := 0
	cfg := Settings{
		Provider: &ProviderSettings{
			MaxRetries:     &maxRetries,
			DefaultTimeout: "90s",
		},
	}.withDefaults()
	require.Equal(t, 0, cfg.ProviderMaxRetries())
	require.Equal(t, 90*time.Second, cfg.ProviderDefaultTimeout())
}
