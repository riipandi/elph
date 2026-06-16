package settings

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestParseFooterTokenDisplay(t *testing.T) {
	require.Equal(t, FooterTokenPercentage, ParseFooterTokenDisplay("percentage"))
	require.Equal(t, FooterTokenBoth, ParseFooterTokenDisplay("both"))
	require.Equal(t, FooterTokenCount, ParseFooterTokenDisplay("count"))

	// Case insensitive
	require.Equal(t, FooterTokenPercentage, ParseFooterTokenDisplay("Percentage"))
	require.Equal(t, FooterTokenBoth, ParseFooterTokenDisplay("BOTH"))
	require.Equal(t, FooterTokenCount, ParseFooterTokenDisplay("Count"))

	// Whitespace trimmed
	require.Equal(t, FooterTokenPercentage, ParseFooterTokenDisplay("  percentage  "))

	// Invalid defaults to both
	require.Equal(t, FooterTokenBoth, ParseFooterTokenDisplay("invalid"))
	require.Equal(t, FooterTokenBoth, ParseFooterTokenDisplay(""))
	require.Equal(t, FooterTokenBoth, ParseFooterTokenDisplay("  "))
}

func TestFooterTokenDisplayMode(t *testing.T) {
	tests := []struct {
		input    string
		expected FooterTokenDisplay
	}{
		{"", FooterTokenBoth},
		{"percentage", FooterTokenPercentage},
		{"both", FooterTokenBoth},
		{"count", FooterTokenCount},
	}
	for _, tt := range tests {
		s := Settings{FooterTokenDisplay: tt.input}
		require.Equal(t, tt.expected, s.FooterTokenDisplayMode())
	}
}
