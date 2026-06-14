package provider

import (
	"encoding/json"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestParseThinkingLevelMap(t *testing.T) {
	raw := map[string]json.RawMessage{
		"off":     json.RawMessage(`null`),
		"high":    json.RawMessage(`"max"`),
		"minimal": json.RawMessage(`"1024"`),
	}
	parsed := ParseThinkingLevelMap(raw)
	require.Equal(t, ThinkingMapUnsupported, parsed[constants.ThinkingOff].State)
	require.Equal(t, ThinkingMapExplicit, parsed[constants.ThinkingHigh].State)
	require.Equal(t, "max", parsed[constants.ThinkingHigh].Value)
	require.Equal(t, ThinkingMapExplicit, parsed[constants.ThinkingMinimal].State)
	require.Equal(t, "1024", parsed[constants.ThinkingMinimal].Value)
}

func TestResolveThinkingAnthropicBudget(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, constants.ThinkingMedium, nil)
	require.True(t, cfg.Enabled)
	require.Equal(t, 10240, cfg.BudgetTokens)
	require.False(t, cfg.Adaptive)
}

func TestResolveThinkingAnthropicAdaptive(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
		Compat: Compat{
			ForceAdaptiveThinking: true,
		},
		ThinkingLevelMap: map[constants.ThinkingLevel]ThinkingMapValue{
			constants.ThinkingXHigh: {State: ThinkingMapExplicit, Value: "max"},
		},
	}
	cfg := ResolveThinking(model, constants.ThinkingXHigh, nil)
	require.True(t, cfg.Adaptive)
	require.Equal(t, "max", cfg.AdaptiveEffort)
}

func TestResolveThinkingOpenAIReasoningEffort(t *testing.T) {
	model := ResolvedModel{
		API:       APIOpenAICompletions,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, constants.ThinkingLow, nil)
	require.True(t, cfg.Enabled)
	require.Equal(t, "low", cfg.ReasoningEffort)
}

func TestResolveThinkingOpenCodeGatewayUsesEnableThinking(t *testing.T) {
	model := ResolvedModel{
		API:       APIOpenAICompletions,
		Reasoning: true,
		Compat: Compat{
			ThinkingFormat:          string(ThinkingFormatQwen),
			SupportsReasoningEffort: compatBool(false),
		},
	}
	cfg := ResolveThinking(model, constants.ThinkingHigh, nil)
	require.True(t, cfg.Enabled)
	require.True(t, cfg.EnableThinking)
	require.Equal(t, ThinkingFormatQwen, cfg.ThinkingFormat)
	require.Empty(t, cfg.ReasoningEffort)
}

func TestApplyGatewayThinkingCompatOpenCodeGo(t *testing.T) {
	cfg := ApplyGatewayThinkingCompat("opencode-go", FileConfig{
		BaseURL: OpenCodeGoBaseURL,
	})
	require.Equal(t, string(ThinkingFormatQwen), cfg.Compat.ThinkingFormat)
	require.NotNil(t, cfg.Compat.SupportsReasoningEffort)
	require.False(t, *cfg.Compat.SupportsReasoningEffort)
}

func TestResolveThinkingOpenRouterFormat(t *testing.T) {
	model := ResolvedModel{
		API:       APIOpenAICompletions,
		Reasoning: true,
		Compat: Compat{
			ThinkingFormat: string(ThinkingFormatOpenRouter),
		},
	}
	cfg := ResolveThinking(model, constants.ThinkingHigh, nil)
	require.Equal(t, ThinkingFormatOpenRouter, cfg.ThinkingFormat)
	require.Equal(t, "high", cfg.ReasoningEffort)
}

func TestClampThinkingLevelSkipsUnsupported(t *testing.T) {
	model := ResolvedModel{
		Reasoning: true,
		ThinkingLevelMap: map[constants.ThinkingLevel]ThinkingMapValue{
			constants.ThinkingOff:     {State: ThinkingMapUnsupported},
			constants.ThinkingMinimal: {State: ThinkingMapUnsupported},
			constants.ThinkingLow:     {State: ThinkingMapUnsupported},
			constants.ThinkingMedium:  {State: ThinkingMapUnsupported},
		},
	}
	require.Equal(t, constants.ThinkingHigh, ClampThinkingLevel(constants.ThinkingLow, model))
}

func TestNextSupportedThinkingLevelSkipsNullLevels(t *testing.T) {
	model := ResolvedModel{
		Reasoning: true,
		ThinkingLevelMap: map[constants.ThinkingLevel]ThinkingMapValue{
			constants.ThinkingMinimal: {State: ThinkingMapUnsupported},
			constants.ThinkingLow:     {State: ThinkingMapUnsupported},
		},
	}
	require.Equal(t, constants.ThinkingMedium, NextSupportedThinkingLevel(constants.ThinkingOff, model))
}

func TestResolveThinkingUsesCustomBudgets(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, constants.ThinkingLow, map[string]int{"low": 2048})
	require.Equal(t, 2048, cfg.BudgetTokens)
}

func TestResolveThinkingOffDisables(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, constants.ThinkingOff, nil)
	require.False(t, cfg.Enabled)
}
