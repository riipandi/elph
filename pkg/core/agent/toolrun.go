package agent

import (
	"encoding/json"
	"fmt"
	"strings"
)

// ToolRunResult is the outcome of executing one provider-native tool call.
type ToolRunResult struct {
	Output    string
	Err       error
	Cancelled bool
}

// ToolResultMessage formats a tool result for provider follow-up messages.
func ToolResultMessage(result ToolRunResult) string {
	if result.Cancelled {
		if trimmed := strings.TrimSpace(result.Output); trimmed != "" {
			return trimmed + "\n(cancelled)"
		}
		return "(cancelled)"
	}
	if result.Err != nil {
		var b strings.Builder
		b.WriteString("Tool error: ")
		b.WriteString(result.Err.Error())
		if trimmed := strings.TrimSpace(result.Output); trimmed != "" {
			b.WriteString("\n")
			b.WriteString(trimmed)
		}
		return b.String()
	}
	if trimmed := strings.TrimSpace(result.Output); trimmed == "" {
		return "(no output)"
	}
	return strings.TrimRight(result.Output, "\n")
}

// ParseToolArguments decodes provider tool arguments.
func ParseToolArguments(raw json.RawMessage) (map[string]any, error) {
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
