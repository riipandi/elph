package schema

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool/catalog"
	"github.com/stretchr/testify/require"
)

func TestIsProviderExposed(t *testing.T) {
	require.True(t, IsProviderExposed(catalog.Read))
	require.True(t, IsProviderExposed(catalog.Grep))
	require.True(t, IsProviderExposed(catalog.Glob))

	require.True(t, IsProviderExposed(catalog.WebSearch))
	require.True(t, IsProviderExposed(catalog.FetchURL))
	require.True(t, IsProviderExposed(catalog.CodeSearch))
	require.True(t, IsProviderExposed(catalog.Write))
	require.True(t, IsProviderExposed(catalog.Edit))
	require.True(t, IsProviderExposed(catalog.ReadMediaFile))
	require.True(t, IsProviderExposed(catalog.Bash))
	require.True(t, IsProviderExposed(catalog.AskUser))
	require.True(t, IsProviderExposed(catalog.Skill))
	require.False(t, IsProviderExposed("unknown"))
}

func TestProviderDefinitionsExecutableTools(t *testing.T) {
	defs := ProviderDefinitions()
	require.Len(t, defs, 12)

	names := make([]string, len(defs))
	for i, def := range defs {
		names[i] = def.Name
		require.NotEmpty(t, def.Description)
		require.NotEmpty(t, def.Parameters)
	}
	require.ElementsMatch(t, []string{
		catalog.Read, catalog.Write, catalog.Edit, catalog.Grep, catalog.Glob,
		catalog.ReadMediaFile, catalog.FetchURL, catalog.WebSearch, catalog.CodeSearch,
		catalog.AskUser, catalog.Skill, catalog.Bash,
	}, names)
}

func TestBashAndAskUserSchemas(t *testing.T) {
	bashSchema, ok := ProviderSchema(catalog.Bash)
	require.True(t, ok)
	require.Equal(t, "object", bashSchema["type"])
	require.True(t, IsProviderExposed(catalog.Bash))

	askSchema, ok := ProviderSchema(catalog.AskUser)
	require.True(t, ok)
	require.Equal(t, "object", askSchema["type"])
	require.True(t, IsProviderExposed(catalog.AskUser))
}

func TestFilterProviderTools(t *testing.T) {
	filtered := FilterProviderTools([]provider.ToolDefinition{
		{Name: catalog.Read},
		{Name: catalog.Grep},
		{Name: catalog.WebSearch},
		{Name: catalog.Write},
	})
	require.Len(t, filtered, 4)
	require.Equal(t, catalog.Read, filtered[0].Name)
	require.Equal(t, catalog.Grep, filtered[1].Name)
	require.Equal(t, catalog.WebSearch, filtered[2].Name)
	require.Equal(t, catalog.Write, filtered[3].Name)
}
