package agent

import (
	"context"
	"time"

	"github.com/riipandi/elph/pkg/ai/protocol"
)

// ToolExecuteFunc runs one provider-native tool invocation.
type ToolExecuteFunc func(ctx context.Context, name string, args map[string]any) ToolRunResult

// ToolExecuteStreamFunc runs a tool and streams stdout/stderr chunks to onChunk.
type ToolExecuteStreamFunc func(ctx context.Context, call protocol.ToolCall, args map[string]any, onChunk func(string)) ToolRunResult

// TurnOptions configures a single agent turn.
type TurnOptions struct {
	SystemPrompt           string
	UserPrompt             string
	Model                  string
	Provider               protocol.Provider
	ShowThinking           bool
	Thinking               protocol.ThinkingConfig
	Compat                 protocol.Compat
	ToolsEnabled           bool
	WorkDir                string
	Messages               []protocol.ChatMessage
	UserImages             []protocol.ImageAttachment
	Tools                  []protocol.ToolDefinition
	ExecuteTool            ToolExecuteFunc
	ExecuteToolStream      ToolExecuteStreamFunc
	InteractTool           ToolInteractFunc
	SkipToolApproval       bool          // brave mode — skip approval dialogs for requires-approval tools
	LogProvider            TurnLogFunc   // optional provider/tool trace (requests log)
	ProviderMaxRetries     int           // retriable failures to retry (0 = default)
	ProviderDefaultTimeout time.Duration // provider inactivity limit (0 = default)
	MaxToolIterations      int           // max autonomous tool rounds per turn (0 = default MaxToolIterationsDefault)
}

// ProviderRetryConfig returns retry settings for upstream provider calls.
func (o TurnOptions) ProviderRetryConfig() ProviderRetryConfig {
	return ProviderRetryConfig{
		MaxRetries:         o.ProviderMaxRetries,
		StreamStallTimeout: o.ProviderDefaultTimeout,
	}
}
