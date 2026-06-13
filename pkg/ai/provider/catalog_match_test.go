package provider

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestCatalogMatchModel(t *testing.T) {
	catalog := Catalog{
		Providers: []RegisteredProvider{{
			ID: "opencode",
			Models: []ResolvedModel{
				{ID: "model-a", Name: "Model A", ProviderID: "opencode", ProviderName: "OpenCode"},
				{ID: "model-b", Name: "Model B", ProviderID: "opencode", ProviderName: "OpenCode"},
			},
		}},
	}

	_, model, ok := catalog.MatchModel("opencode/model-b")
	require.True(t, ok)
	require.Equal(t, "model-b", model.ID)

	_, model, ok = catalog.MatchModel("Model A")
	require.True(t, ok)
	require.Equal(t, "model-a", model.ID)

	_, model, ok = catalog.MatchModel("model-b")
	require.True(t, ok)
	require.Equal(t, "model-b", model.ID)
}
