package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFetchOpenCodeModels(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/models", r.URL.Path)
		require.NoError(t, json.NewEncoder(w).Encode(OpenCodeModelsResponse{
			Object: "list",
			Data: []OpenCodeModelEntry{
				{ID: "claude-sonnet-4-6"},
				{ID: "gpt-5.4"},
			},
		}))
	}))
	defer srv.Close()

	ids, err := FetchOpenCodeModels(context.Background(), srv.Client(), srv.URL)
	require.NoError(t, err)
	require.Equal(t, []string{"claude-sonnet-4-6", "gpt-5.4"}, ids)
}
