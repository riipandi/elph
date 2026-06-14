package command

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFuzzyScoreSubsequence(t *testing.T) {
	t.Run("delegates to internal/fuzzy", func(t *testing.T) {
		cmd, ok := Get("help", Context{})
		require.True(t, ok)
		require.Positive(t, commandScore("h", cmd))
		require.Equal(t, -1, commandScore("zzz", cmd))
	})
}

func TestCommandScoreUsesAliases(t *testing.T) {
	cmd, ok := Get("exit", Context{})
	require.True(t, ok)
	require.Positive(t, commandScore("quit", cmd))
	require.Positive(t, commandScore("qt", cmd))
}
