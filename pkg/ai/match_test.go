package ai

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestMatchModel(t *testing.T) {
	catalog := provider.Catalog{
		Providers: []provider.RegisteredProvider{{
			ID: "opencode",
			Models: []provider.ResolvedModel{
				{ID: "model-a", Name: "Model A", ProviderID: "opencode", ProviderName: "OpenCode"},
				{ID: "model-b", Name: "Model B", ProviderID: "opencode", ProviderName: "OpenCode"},
			},
		}},
	}

	_, model, ok := MatchModel(catalog, "opencode/model-b")
	require.True(t, ok)
	require.Equal(t, "model-b", model.ID)

	_, model, ok = MatchModel(catalog, "Model A")
	require.True(t, ok)
	require.Equal(t, "model-a", model.ID)

	_, model, ok = MatchModel(catalog, "model-b")
	require.True(t, ok)
	require.Equal(t, "model-b", model.ID)
}