package ai

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestBuildSelectorGroups(t *testing.T) {
	catalog := provider.Catalog{
		Providers: []provider.RegisteredProvider{
			{
				ID:     "alpha",
				Config: provider.FileConfig{Name: "Alpha"},
				Models: []provider.ResolvedModel{
					{ID: "a1", Name: "Alpha One", ProviderID: "alpha", ProviderName: "Alpha"},
				},
			},
			{
				ID:     "beta",
				Config: provider.FileConfig{Name: "Beta"},
				Models: []provider.ResolvedModel{
					{ID: "b1", Name: "Beta One", ProviderID: "beta", ProviderName: "Beta"},
					{ID: "b2", Name: "Beta Two", ProviderID: "beta", ProviderName: "Beta"},
				},
			},
		},
	}

	groups, flat := BuildSelectorGroups(catalog, "")
	require.Len(t, groups, 2)
	require.Len(t, flat, 3)
	require.Equal(t, "Alpha", groups[0].ProviderName)
	require.Equal(t, "a1", groups[0].Models[0].ID)
	require.Equal(t, "b2", groups[1].Models[1].ID)
}

func TestBuildSelectorGroupsFuzzyFilter(t *testing.T) {
	catalog := provider.Catalog{
		Providers: []provider.RegisteredProvider{{
			ID: "opencode",
			Models: []provider.ResolvedModel{
				{ID: "model-a", Name: "Fast Model", ProviderID: "opencode", ProviderName: "OpenCode"},
				{ID: "model-b", Name: "Smart Model", ProviderID: "opencode", ProviderName: "OpenCode"},
			},
		}},
	}

	groups, flat := BuildSelectorGroups(catalog, "smart")
	require.Len(t, groups, 1)
	require.Len(t, flat, 1)
	require.Equal(t, "model-b", flat[0].ID)
}

func TestFlattenSelectorGroups(t *testing.T) {
	groups := []SelectorGroup{
		{ProviderID: "alpha", Models: []provider.ResolvedModel{{ID: "a1"}}},
		{ProviderID: "beta", Models: []provider.ResolvedModel{{ID: "b1"}, {ID: "b2"}}},
	}

	all := FlattenSelectorGroups(groups, "")
	require.Len(t, all, 3)
	require.Equal(t, "a1", all[0].ID)

	beta := FlattenSelectorGroups(groups, "beta")
	require.Len(t, beta, 2)
	require.Equal(t, "b1", beta[0].ID)
}

func TestCycleProviderFilter(t *testing.T) {
	groups := []SelectorGroup{
		{ProviderID: "alpha"},
		{ProviderID: "beta"},
	}

	require.Equal(t, "alpha", CycleProviderFilter("", 1, groups))
	require.Equal(t, "beta", CycleProviderFilter("alpha", 1, groups))
	require.Equal(t, "", CycleProviderFilter("beta", 1, groups))
	require.Equal(t, "beta", CycleProviderFilter("", -1, groups))
}

func TestNormalizeProviderFilter(t *testing.T) {
	groups := []SelectorGroup{{ProviderID: "alpha"}}
	require.Equal(t, "", NormalizeProviderFilter("", groups))
	require.Equal(t, "alpha", NormalizeProviderFilter("alpha", groups))
	require.Equal(t, "", NormalizeProviderFilter("missing", groups))
}

func TestSelectorPickIndex(t *testing.T) {
	flat := []provider.ResolvedModel{
		{ProviderID: "alpha", ID: "a1"},
		{ProviderID: "beta", ID: "b1"},
	}
	require.Equal(t, 1, SelectorPickIndex(flat, "beta", "b1"))
	require.Equal(t, 0, SelectorPickIndex(flat, "missing", "x"))
}
