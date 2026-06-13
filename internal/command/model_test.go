package command

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func writeProviderFile(t *testing.T, dir, name, body string) {
	t.Helper()
	require.NoError(t, os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644))
}

func TestModelOpensSelector(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "model-a", "name": "Model A"}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	ctx := &Context{Catalog: catalog}
	output := modelHandler(ctx, "")
	require.Empty(t, output)
	require.True(t, ctx.pendingOpenSelector)
	require.Equal(t, catalog, ctx.selectorCatalog)
	require.Empty(t, ctx.selectorQuery)
}

func TestModelOpensSelectorWithQuery(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "test",
		"models": [{"id": "model-a", "name": "Model A"}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	ctx := &Context{Catalog: catalog}
	modelHandler(ctx, "model-a")
	require.True(t, ctx.pendingOpenSelector)
	require.Equal(t, "model-a", ctx.selectorQuery)
}

func TestExecuteModelReturnsSelector(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [{"id": "model-a", "name": "Model A"}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	result := Execute("/model smart", Context{Catalog: catalog})
	require.True(t, result.OK)
	require.True(t, result.OpenModelSelector)
	require.Equal(t, "smart", result.SelectorQuery)
	require.Len(t, result.SelectorCatalog.Providers, 1)
}
