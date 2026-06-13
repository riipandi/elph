package provider

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestResolveFromEnvWithoutKeys(t *testing.T) {
	t.Setenv(anthropicAPIKeyEnv, "")
	t.Setenv(openAIAPIKeyEnv, "")
	t.Setenv(deepSeekAPIKeyEnv, "")
	t.Setenv(elphModelEnv, "")

	cfg := ResolveFromEnv()
	require.Nil(t, cfg.Provider)
	require.Empty(t, cfg.ID)
}

func TestResolveFromEnvPrefersAnthropic(t *testing.T) {
	t.Setenv(anthropicAPIKeyEnv, "anthropic-key")
	t.Setenv(openAIAPIKeyEnv, "openai-key")
	t.Setenv(deepSeekAPIKeyEnv, "")
	t.Setenv(elphModelEnv, "")

	cfg := ResolveFromEnv()
	require.NotNil(t, cfg.Provider)
	require.Equal(t, IDAnthropic, cfg.ID)
}