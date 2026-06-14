package openrouter

import (
	openaisdk "github.com/openai/openai-go/v3"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
	elphopenai "github.com/riipandi/elph/pkg/ai/providers/openai"
	"github.com/riipandi/elph/pkg/ai/providers/openaicompat"
)

func openRouterHooks() elphopenai.Hooks {
	base := openaicompat.CompatHooks()
	orig := base.PrepareParams
	base.PrepareParams = func(req provider.TurnRequest, params *openaisdk.ChatCompletionNewParams) {
		if orig != nil {
			orig(req, params)
		}
		prepareOpenRouterParams(req, params)
	}
	return base
}

func prepareOpenRouterParams(req provider.TurnRequest, params *openaisdk.ChatCompletionNewParams) {
	if !req.Thinking.Enabled {
		return
	}
	if req.Thinking.ThinkingFormat != provider.ThinkingFormatOpenRouter {
		return
	}
	if req.Thinking.ReasoningEffort == "" {
		return
	}
	params.SetExtraFields(map[string]any{
		"reasoning": map[string]any{"effort": req.Thinking.ReasoningEffort},
	})
}
