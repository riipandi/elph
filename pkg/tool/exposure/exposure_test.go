package exposure

import (
	"testing"

	"github.com/riipandi/elph/pkg/tool/catalog"
	"github.com/stretchr/testify/require"
)

func TestResolveNameBuiltin(t *testing.T) {
	name, ok := ResolveName("websearch")
	require.True(t, ok)
	require.Equal(t, catalog.WebSearch, name)
}

func TestResolveNameUnknown(t *testing.T) {
	name, ok := ResolveName("mcp_figma_search")
	require.False(t, ok)
	require.Equal(t, "Mcp_figma_search", name)
}

func TestIsExecutableKnownBuiltin(t *testing.T) {
	require.True(t, IsExecutable(catalog.Read))
	require.True(t, IsExecutable(catalog.Grep))
	require.True(t, IsExecutable(catalog.Glob))
	require.True(t, IsExecutable(catalog.Bash))
	require.True(t, IsExecutable(catalog.AskUser))
	require.True(t, IsExecutable(catalog.Write))
	require.True(t, IsExecutable(catalog.Edit))
	require.True(t, IsExecutable(catalog.ReadMediaFile))
	require.True(t, IsExecutable(catalog.WebSearch))
	require.True(t, IsExecutable(catalog.FetchURL))
	require.True(t, IsExecutable(catalog.CodeSearch))
	require.False(t, IsExecutable("unknown"))
}
