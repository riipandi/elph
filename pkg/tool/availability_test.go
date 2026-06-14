package tool

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestResolveNameBuiltin(t *testing.T) {
	name, ok := ResolveName("websearch")
	require.True(t, ok)
	require.Equal(t, WebSearch, name)
}

func TestResolveNameUnknown(t *testing.T) {
	name, ok := ResolveName("mcp_figma_search")
	require.False(t, ok)
	require.Equal(t, "Mcp_figma_search", name)
}

func TestIsExecutableKnownBuiltin(t *testing.T) {
	require.True(t, IsExecutable(Read))
	require.True(t, IsExecutable(Grep))
	require.True(t, IsExecutable(Glob))
	require.False(t, IsExecutable(Write))
	require.False(t, IsExecutable(WebSearch))
	require.False(t, IsExecutable("unknown"))
}
