package openai

import (
	openaisdk "github.com/openai/openai-go/v3"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

// Hooks customize OpenAI chat completion request/response handling.
type Hooks struct {
	PrepareParams   func(req provider.TurnRequest, params *openaisdk.ChatCompletionNewParams)
	ChatMessages    func(systemPrompt string, messages []provider.ChatMessage, thinking provider.ThinkingConfig, compat provider.Compat) []openaisdk.ChatCompletionMessageParamUnion
	ChoiceReasoning func(choice openaisdk.ChatCompletionChoice) string
	StreamReasoning func(delta openaisdk.ChatCompletionChunkChoiceDelta) string
}

// DefaultHooks returns identity hooks with standard message mapping.
func DefaultHooks() Hooks {
	return Hooks{
		ChatMessages:    chatMessages,
		ChoiceReasoning: choiceReasoningText,
		StreamReasoning: streamReasoningText,
	}
}
