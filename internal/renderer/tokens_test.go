package renderer

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatTokenCount(t *testing.T) {
	require.Equal(t, "128k", formatTokenCount(128000))
	require.Equal(t, "200k", formatTokenCount(200000))
	require.Equal(t, "262k", formatTokenCount(262144))
	require.Equal(t, "16k", formatTokenCount(16384))
	require.Equal(t, "—", formatTokenCount(0))
}

// TestFooterTokenUsageLabel tests the different display modes.
func TestFooterTokenUsageLabel(t *testing.T) {
	m := New()
	m.contextWindow = 262144
	m.tokensUsed = 131072 // 50% of 262144

	// Default: percentage mode — shows percentage | context window
	m.footerTokenDisplay = "percentage"
	require.Equal(t, "0.0% | 262k", m.footerTokenUsageLabel(0.0, 0))
	require.Equal(t, "50.0% | 262k", m.footerTokenUsageLabel(0.5, 131072))
	require.Equal(t, "100.0% | 262k", m.footerTokenUsageLabel(1.0, 262144))

	// Both mode: same as percentage
	m.footerTokenDisplay = "both"
	require.Equal(t, "0.0% | 262k", m.footerTokenUsageLabel(0.0, 0))
	require.Equal(t, "50.0% | 262k", m.footerTokenUsageLabel(0.5, 131072))

	// Count mode: used tokens | context window
	m.footerTokenDisplay = "count"
	require.Equal(t, "— | 262k", m.footerTokenUsageLabel(0.0, 0))
	require.Equal(t, "131k | 262k", m.footerTokenUsageLabel(0.5, 131072))
	require.Equal(t, "262k | 262k", m.footerTokenUsageLabel(1.0, 262144))

	// Invalid mode defaults to percentage
	m.footerTokenDisplay = "invalid"
	require.Equal(t, "0.0% | 262k", m.footerTokenUsageLabel(0.0, 0))
}
