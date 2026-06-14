package agent

import (
	"encoding/json"
	"fmt"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// ToolRunResult is the outcome of executing one provider-native tool call.
type ToolRunResult struct {
	Output    string
	Err       error
	Cancelled bool
}

// ToolResultMessage formats a bounded tool result for provider follow-up messages.
func ToolResultMessage(result ToolRunResult) string {
	return toolResultMessageLimited(result)
}

// ParseToolArguments decodes provider tool arguments.
func ParseToolArguments(raw json.RawMessage) (map[string]any, error) {
	raw = provider.NormalizeToolArguments(raw)
	if len(raw) == 0 {
		return map[string]any{}, nil
	}
	var args map[string]any
	if err := json.Unmarshal(raw, &args); err != nil {
		return nil, fmt.Errorf("decode tool arguments: %w", err)
	}
	if args == nil {
		args = map[string]any{}
	}
	return args, nil
}
