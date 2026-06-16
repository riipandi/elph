package provider

import (
	"encoding/json"
	"testing"

	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/stretchr/testify/require"
)

func TestParseThinkingLevelMap(t *testing.T) {
	raw := map[string]json.RawMessage{
		"off":     json.RawMessage(`null`),
		"high":    json.RawMessage(`"max"`),
		"minimal": json.RawMessage(`"1024"`),
	}
	parsed := ParseThinkingLevelMap(raw)
	require.Equal(t, ThinkingMapUnsupported, parsed[protocol.ThinkingOff].State)
	require.Equal(t, ThinkingMapExplicit, parsed[protocol.ThinkingHigh].State)
	require.Equal(t, "max", parsed[protocol.ThinkingHigh].Value)
	require.Equal(t, ThinkingMapExplicit, parsed[protocol.ThinkingMinimal].State)
	require.Equal(t, "1024", parsed[protocol.ThinkingMinimal].Value)
}

func TestResolveThinkingAnthropicBudget(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, protocol.ThinkingMedium, nil)
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
		ThinkingLevelMap: map[protocol.ThinkingLevel]ThinkingMapValue{
			protocol.ThinkingXHigh: {State: ThinkingMapExplicit, Value: "max"},
		},
	}
	cfg := ResolveThinking(model, protocol.ThinkingXHigh, nil)
	require.True(t, cfg.Adaptive)
	require.Equal(t, "max", cfg.AdaptiveEffort)
}

func TestResolveThinkingOpenAIReasoningEffort(t *testing.T) {
	model := ResolvedModel{
		API:       APIOpenAICompletions,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, protocol.ThinkingLow, nil)
	require.True(t, cfg.Enabled)
	require.Equal(t, "low", cfg.ReasoningEffort)
}

func TestResolveThinkingOpenCodeGatewayUsesEnableThinking(t *testing.T) {
	model := ResolvedModel{
		API:       APIOpenAICompletions,
		Reasoning: true,
		Compat: Compat{
			ThinkingFormat:          string(ThinkingFormatQwen),
			SupportsReasoningEffort: new(false),
		},
	}
	cfg := ResolveThinking(model, protocol.ThinkingHigh, nil)
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
	cfg := ResolveThinking(model, protocol.ThinkingHigh, nil)
	require.Equal(t, ThinkingFormatOpenRouter, cfg.ThinkingFormat)
	require.Equal(t, "high", cfg.ReasoningEffort)
}

func TestClampThinkingLevelSkipsUnsupported(t *testing.T) {
	model := ResolvedModel{
		Reasoning: true,
		ThinkingLevelMap: map[protocol.ThinkingLevel]ThinkingMapValue{
			protocol.ThinkingOff:     {State: ThinkingMapUnsupported},
			protocol.ThinkingMinimal: {State: ThinkingMapUnsupported},
			protocol.ThinkingLow:     {State: ThinkingMapUnsupported},
			protocol.ThinkingMedium:  {State: ThinkingMapUnsupported},
		},
	}
	require.Equal(t, protocol.ThinkingHigh, ClampThinkingLevel(protocol.ThinkingLow, model))
}

func TestNextSupportedThinkingLevelSkipsNullLevels(t *testing.T) {
	model := ResolvedModel{
		Reasoning: true,
		ThinkingLevelMap: map[protocol.ThinkingLevel]ThinkingMapValue{
			protocol.ThinkingMinimal: {State: ThinkingMapUnsupported},
			protocol.ThinkingLow:     {State: ThinkingMapUnsupported},
		},
	}
	require.Equal(t, protocol.ThinkingMedium, NextSupportedThinkingLevel(protocol.ThinkingOff, model))
}

func TestResolveThinkingUsesCustomBudgets(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, protocol.ThinkingLow, map[string]int{"low": 2048})
	require.Equal(t, 2048, cfg.BudgetTokens)
}

func TestResolveThinkingOffDisables(t *testing.T) {
	model := ResolvedModel{
		API:       APIAnthropicMessages,
		Reasoning: true,
	}
	cfg := ResolveThinking(model, protocol.ThinkingOff, nil)
	require.False(t, cfg.Enabled)
}
