package template

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestParseArgsRespectsQuotes(t *testing.T) {
	got := ParseArgs(`Button "click handler" disabled`)
	require.Equal(t, []string{"Button", "click handler", "disabled"}, got)
}

func TestSubstituteArgsPositional(t *testing.T) {
	content := "Create component $1 with features: $@"
	got := SubstituteArgs(content, []string{"Button", "onClick", "disabled"})
	require.Equal(t, "Create component Button with features: Button onClick disabled", got)
}

func TestSubstituteArgsDefault(t *testing.T) {
	content := "Summarize in ${1:-7} bullet points."
	got := SubstituteArgs(content, nil)
	require.Equal(t, "Summarize in 7 bullet points.", got)
}

func TestSubstituteArgsSlice(t *testing.T) {
	content := "Use args ${@:2:1} and rest ${@:3}"
	got := SubstituteArgs(content, []string{"a", "b", "c", "d"})
	require.Equal(t, "Use args b and rest c d", got)
}
