package command

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestArgsHintJoinsValues(t *testing.T) {
	got := ArgsHint(openLogArgs)
	require.Equal(t, "requests | system", got)
}

func TestResolveInputMatchesCommandAndArgs(t *testing.T) {
	cmd, argQuery, ok := ResolveInput("/diagnostic:open-log requests", Context{})
	require.True(t, ok)
	require.Equal(t, DiagnosticOpenLog, cmd.Name)
	require.Equal(t, "requests", argQuery)
}

func TestArgExactMatch(t *testing.T) {
	require.True(t, ArgExactMatch(openLogArgs, "system"))
	require.False(t, ArgExactMatch(openLogArgs, "sys"))
}

func TestSuggestArgsFiltersByPrefix(t *testing.T) {
	cmd, ok := Get(DiagnosticOpenLog, Context{})
	require.True(t, ok)

	got := SuggestArgs(cmd, Context{}, "sys")
	require.Len(t, got, 1)
	require.Equal(t, "system", got[0].Value)
}

func TestCompleteInputAddsSpaceForArgCommands(t *testing.T) {
	cmd, ok := Get(DiagnosticOpenLog, Context{})
	require.True(t, ok)
	require.Equal(t, "/diagnostic:open-log ", CompleteInput(cmd, Context{}))
}

func TestArgChoiceIndexExactMatch(t *testing.T) {
	require.Equal(t, 1, ArgChoiceIndex(openLogArgs, "system"))
}

func TestCompleteArgInput(t *testing.T) {
	cmd, ok := Get(DiagnosticOpenLog, Context{})
	require.True(t, ok)
	require.Equal(t, "/diagnostic:open-log system", CompleteArgInput(cmd, openLogArgs[1]))
}
