package settings

import (
	"os"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestSessionDefaults(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, constants.ModeBuild, cfg.AgentMode())
	require.Equal(t, constants.ThinkingHigh, cfg.ThinkingLevel())
	require.Empty(t, cfg.ActiveProviderID())
	require.Empty(t, cfg.ActiveModelID())
}

func TestSetActiveModelPersists(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, SetActiveModel("anthropic", "claude-sonnet-4"))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, "anthropic", cfg.ActiveProviderID())
	require.Equal(t, "claude-sonnet-4", cfg.ActiveModelID())
}

func TestSetAgentModePersists(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, SetAgentMode(constants.ModePlan))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, constants.ModePlan, cfg.AgentMode())
}

func TestSetThinkingLevelPersists(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, SetThinkingLevel(constants.ThinkingLow))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, constants.ThinkingLow, cfg.ThinkingLevel())
}

func TestSessionNormalizesInvalidValues(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, Save(Settings{
		Session: SessionSettings{
			AgentMode:     "nope",
			ThinkingLevel: "nope",
		},
	}))

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, constants.ModeBuild, cfg.AgentMode())
	require.Equal(t, constants.ThinkingHigh, cfg.ThinkingLevel())
}

func TestSessionRoundTripPreservesOtherFields(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	disabled := false
	require.NoError(t, Save(Settings{
		Models: ModelsSettings{
			SyncInterval: "6h",
		},
		ShowThinking: &disabled,
	}))

	require.NoError(t, SetActiveModel("openai", "gpt-4o"))
	require.NoError(t, SetAgentMode(constants.ModeAsk))
	require.NoError(t, SetThinkingLevel(constants.ThinkingMedium))

	cfg, err := Load()
	require.NoError(t, err)
	require.False(t, cfg.ShowThinkingEnabled())
	require.Equal(t, "6h", cfg.Models.SyncInterval)
	require.Equal(t, "openai", cfg.ActiveProviderID())
	require.Equal(t, "gpt-4o", cfg.ActiveModelID())
	require.Equal(t, constants.ModeAsk, cfg.AgentMode())
	require.Equal(t, constants.ThinkingMedium, cfg.ThinkingLevel())

	raw, err := os.ReadFile(mustPath(t))
	require.NoError(t, err)
	require.Contains(t, string(raw), `"providerId": "openai"`)
	require.Contains(t, string(raw), `"agentMode": "ask"`)
}

func mustPath(t *testing.T) string {
	t.Helper()
	path, err := Path()
	require.NoError(t, err)
	return path
}