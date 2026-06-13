package provider

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFetchLiveModelsWithAuthHeader(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Equal(t, "/models", r.URL.Path)
		require.Equal(t, "Bearer sk-test", r.Header.Get("Authorization"))
		require.NoError(t, json.NewEncoder(w).Encode(CompatibleModelsResponse{
			Object: "list",
			Data: []CompatibleModelEntry{
				{ID: "deepseek-chat"},
				{ID: "deepseek-reasoner"},
			},
		}))
	}))
	defer srv.Close()

	ids, err := FetchLiveModels(context.Background(), srv.Client(), LiveModelsOptions{
		BaseURL:    srv.URL,
		APIKey:     "sk-test",
		AuthHeader: true,
	})
	require.NoError(t, err)
	require.Equal(t, []string{"deepseek-chat", "deepseek-reasoner"}, ids)
}

func TestFetchLiveModelsWithoutAuth(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		require.Empty(t, r.Header.Get("Authorization"))
		require.NoError(t, json.NewEncoder(w).Encode(CompatibleModelsResponse{
			Object: "list",
			Data:   []CompatibleModelEntry{{ID: "big-pickle"}},
		}))
	}))
	defer srv.Close()

	ids, err := FetchLiveModels(context.Background(), srv.Client(), LiveModelsOptions{BaseURL: srv.URL})
	require.NoError(t, err)
	require.Equal(t, []string{"big-pickle"}, ids)
}
