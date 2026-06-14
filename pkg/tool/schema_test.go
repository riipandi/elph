package tool

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestIsProviderExposed(t *testing.T) {
	require.True(t, IsProviderExposed(Read))
	require.True(t, IsProviderExposed(Grep))
	require.True(t, IsProviderExposed(Glob))

	require.False(t, IsProviderExposed(WebSearch))
	require.False(t, IsProviderExposed(FetchURL))
	require.True(t, IsProviderExposed(Write))
	require.True(t, IsProviderExposed(Edit))
	require.True(t, IsProviderExposed(Bash))
	require.True(t, IsProviderExposed(AskUser))
	require.False(t, IsProviderExposed("unknown"))
}

func TestProviderDefinitionsExecutableTools(t *testing.T) {
	defs := ProviderDefinitions()
	require.Len(t, defs, 7)

	names := make([]string, len(defs))
	for i, def := range defs {
		names[i] = def.Name
		require.NotEmpty(t, def.Description)
		require.NotEmpty(t, def.Parameters)
	}
	require.ElementsMatch(t, []string{Read, Write, Edit, Grep, Glob, AskUser, Bash}, names)
}

func TestBashAndAskUserSchemas(t *testing.T) {
	bashSchema, ok := providerSchema(Bash)
	require.True(t, ok)
	require.Equal(t, "object", bashSchema["type"])
	require.True(t, IsProviderExposed(Bash))

	askSchema, ok := providerSchema(AskUser)
	require.True(t, ok)
	require.Equal(t, "object", askSchema["type"])
	require.True(t, IsProviderExposed(AskUser))
}

func TestFilterProviderTools(t *testing.T) {
	filtered := FilterProviderTools([]provider.ToolDefinition{
		{Name: Read},
		{Name: Grep},
		{Name: WebSearch},
		{Name: Write},
	})
	require.Len(t, filtered, 3)
	require.Equal(t, Read, filtered[0].Name)
	require.Equal(t, Grep, filtered[1].Name)
	require.Equal(t, Write, filtered[2].Name)
}
