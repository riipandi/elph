package agent

import (
	"context"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// ToolExecuteFunc runs one provider-native tool invocation.
type ToolExecuteFunc func(ctx context.Context, name string, args map[string]any) ToolRunResult

// TurnOptions configures a single agent turn.
type TurnOptions struct {
	SystemPrompt string
	UserPrompt   string
	Model        string
	Provider     provider.Provider
	ShowThinking bool
	Thinking     provider.ThinkingConfig
	Compat       provider.Compat
	ToolsEnabled bool
	WorkDir      string
	Messages     []provider.ChatMessage
	Tools        []provider.ToolDefinition
	ExecuteTool  ToolExecuteFunc
}
