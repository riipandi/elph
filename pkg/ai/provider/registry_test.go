package provider

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestResolveWithoutProviders(t *testing.T) {
	t.Setenv("ELPH_PROVIDERS_DIR", t.TempDir())
	t.Setenv("ELPH_PROVIDER", "")
	t.Setenv("ELPH_MODEL", "")

	cfg := Resolve()
	require.Nil(t, cfg.Provider)
	require.Empty(t, cfg.ProviderID)
}

func TestResolveUsesFirstConfiguredProvider(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "first.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "",
		"models": [{"id": "m1", "name": "First"}]
	}`)
	writeProviderFile(t, dir, "second.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$SECOND_KEY",
		"models": [{"id": "m2", "name": "Second"}]
	}`)
	t.Setenv("ELPH_PROVIDERS_DIR", dir)
	t.Setenv("SECOND_KEY", "secret")
	t.Setenv("ELPH_PROVIDER", "")
	t.Setenv("ELPH_MODEL", "")

	cfg := Resolve()
	require.NotNil(t, cfg.Provider)
	require.Equal(t, "second", cfg.ProviderID)
	require.Equal(t, "m2", cfg.ModelID)
	require.Equal(t, "Second", cfg.ModelName)
}

func TestResolveUsesSavedActiveModel(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "first.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$FIRST_KEY",
		"models": [{"id": "m1", "name": "First"}]
	}`)
	writeProviderFile(t, dir, "second.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$SECOND_KEY",
		"models": [{"id": "m2", "name": "Second"}]
	}`)
	t.Setenv("ELPH_PROVIDERS_DIR", dir)
	t.Setenv("FIRST_KEY", "secret")
	t.Setenv("SECOND_KEY", "secret")
	t.Setenv("ELPH_PROVIDER", "")
	t.Setenv("ELPH_MODEL", "")

	cfg := ResolveActive("first", "m1")
	require.NotNil(t, cfg.Provider)
	require.Equal(t, "first", cfg.ProviderID)
	require.Equal(t, "m1", cfg.ModelID)
	require.Equal(t, "First", cfg.ModelName)
}

func TestResolveEnvOverridesSavedActiveModel(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "first.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$FIRST_KEY",
		"models": [{"id": "m1", "name": "First"}]
	}`)
	writeProviderFile(t, dir, "second.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$SECOND_KEY",
		"models": [{"id": "m2", "name": "Second"}]
	}`)
	t.Setenv("ELPH_PROVIDERS_DIR", dir)
	t.Setenv("FIRST_KEY", "secret")
	t.Setenv("SECOND_KEY", "secret")
	t.Setenv("ELPH_PROVIDER", "second")
	t.Setenv("ELPH_MODEL", "m2")

	cfg := ResolveActive("first", "m1")
	require.Equal(t, "second", cfg.ProviderID)
	require.Equal(t, "m2", cfg.ModelID)
}

func TestResolveFallsBackWhenSavedModelMissing(t *testing.T) {
	dir := t.TempDir()
	writeProviderFile(t, dir, "first.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "$FIRST_KEY",
		"models": [{"id": "m1", "name": "First"}]
	}`)
	t.Setenv("ELPH_PROVIDERS_DIR", dir)
	t.Setenv("FIRST_KEY", "secret")
	t.Setenv("ELPH_PROVIDER", "")
	t.Setenv("ELPH_MODEL", "")

	cfg := ResolveActive("first", "missing")
	require.Equal(t, "first", cfg.ProviderID)
	require.Equal(t, "m1", cfg.ModelID)
}
