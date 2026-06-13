package fuzzy

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestScoreSubsequence(t *testing.T) {
	require.Positive(t, Score("quit", "quit"))
	require.Positive(t, Score("qt", "quit"))
	require.Positive(t, Score("diag", "diagnostic:list-tools"))
	require.Equal(t, -1, Score("zzz", "help"))
}

func TestScoreEmptyQuery(t *testing.T) {
	require.Equal(t, 0, Score("", "help"))
}
