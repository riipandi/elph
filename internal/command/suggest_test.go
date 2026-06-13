package command

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestSuggestDiagnosticFuzzyPrefix(t *testing.T) {
	got := Suggest("diag")
	require.NotEmpty(t, got)
	require.True(t, strings.HasPrefix(got[0].Name, "diagnostic:"))
}

func TestSuggestDebugMatchesDiagnostic(t *testing.T) {
	got := Suggest("debug")
	require.NotEmpty(t, got)
	require.Equal(t, DiagnosticDebug, got[0].Name)
}

func TestSuggestQuitMatchesExit(t *testing.T) {
	got := Suggest("quit")
	require.NotEmpty(t, got)
	require.Equal(t, "exit", got[0].Name)
}

func TestSuggestFuzzyAbbreviation(t *testing.T) {
	got := Suggest("qt")
	require.NotEmpty(t, got)
	require.Equal(t, "exit", got[0].Name)
}

func TestSuggestLimitsResults(t *testing.T) {
	got := Suggest("")
	require.LessOrEqual(t, len(got), maxSuggestions)
}

func TestCompleteInput(t *testing.T) {
	cmd, ok := Get(DiagnosticListTools)
	require.True(t, ok)
	require.Equal(t, "/diagnostic:list-tools", CompleteInput(cmd, Context{}))
}
