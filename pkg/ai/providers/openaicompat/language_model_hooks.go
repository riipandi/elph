package openaicompat

import (
	openaisdk "github.com/openai/openai-go/v3"
	"github.com/openai/openai-go/v3/shared"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
)

func compatHooks() elphopenai.Hooks {
	base := elphopenai.DefaultHooks()
	base.PrepareParams = prepareParams
	return base
}

func prepareParams(req provider.TurnRequest, params *openaisdk.ChatCompletionNewParams) {
	if !req.Thinking.Enabled {
		return
	}
	switch req.Thinking.ThinkingFormat {
	case provider.ThinkingFormatOpenRouter:
		return
	case provider.ThinkingFormatQwen:
		params.SetExtraFields(map[string]any{"enable_thinking": req.Thinking.EnableThinking})
	default:
		if req.Compat.ReasoningEffortSupported() && req.Thinking.ReasoningEffort != "" {
			params.ReasoningEffort = shared.ReasoningEffort(req.Thinking.ReasoningEffort)
		} else if req.Thinking.EnableThinking {
			params.SetExtraFields(map[string]any{"enable_thinking": true})
		}
	}
}
