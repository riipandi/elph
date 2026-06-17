package provider

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestEnsureStarterProvidersCreatesTemplatesOnFreshInstall(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("ELPH_PROVIDERS_DIR", dir)

	result, created, err := EnsureStarterProviders()
	require.NoError(t, err)
	require.True(t, created)
	require.Len(t, result.Created, 7)

	catalog, err := LoadCatalog(dir)
	require.NoError(t, err)
	require.Len(t, catalog.Providers, 7)

	result, created, err = EnsureStarterProviders()
	require.NoError(t, err)
	require.False(t, created)
	require.Empty(t, result.Created)
}

func TestEnsureStarterProvidersBootstrapsMissingDir(t *testing.T) {
	home := t.TempDir()
	providersDir := filepath.Join(home, ".elph", "providers")
	t.Setenv("HOME", home)

	_, created, err := EnsureStarterProviders()
	require.NoError(t, err)
	require.True(t, created)

	info, err := os.Stat(providersDir)
	require.NoError(t, err)
	require.True(t, info.IsDir())
}
