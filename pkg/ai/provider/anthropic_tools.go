package provider

import (
	"encoding/json"
)

type anthropicToolUseBlock struct {
	Type  string         `json:"type"`
	ID    string         `json:"id"`
	Name  string         `json:"name"`
	Input map[string]any `json:"input"`
}

type anthropicResponse struct {
	Content    []json.RawMessage `json:"content"`
	StopReason string            `json:"stop_reason"`
}

func parseAnthropicResponse(raw anthropicResponse) TurnResult {
	var result TurnResult
	for _, blockRaw := range raw.Content {
		var kind struct {
			Type string `json:"type"`
		}
		if err := json.Unmarshal(blockRaw, &kind); err != nil {
			continue
		}
		switch kind.Type {
		case "thinking":
			var block struct {
				Thinking string `json:"thinking"`
			}
			_ = json.Unmarshal(blockRaw, &block)
			if block.Thinking != "" {
				if result.Thinking != "" {
					result.Thinking += "\n"
				}
				result.Thinking += block.Thinking
			}
		case "text":
			var block struct {
				Text string `json:"text"`
			}
			_ = json.Unmarshal(blockRaw, &block)
			if block.Text != "" {
				if result.Content != "" {
					result.Content += "\n"
				}
				result.Content += block.Text
			}
		case "tool_use":
			var block anthropicToolUseBlock
			_ = json.Unmarshal(blockRaw, &block)
			if block.ID == "" || block.Name == "" {
				continue
			}
			input := block.Input
			if input == nil {
				input = map[string]any{}
			}
			args, _ := json.Marshal(input)
			result.ToolCalls = append(result.ToolCalls, ToolCall{
				ID:        block.ID,
				Name:      block.Name,
				Arguments: args,
			})
		}
	}
	switch raw.StopReason {
	case "tool_use":
		result.StopReason = StopReasonToolUse
	default:
		if len(result.ToolCalls) > 0 {
			result.StopReason = StopReasonToolUse
		} else {
			result.StopReason = StopReasonEndTurn
		}
	}
	return result
}

func anthropicResultValid(result TurnResult) bool {
	return result.Thinking != "" || result.Content != "" || len(result.ToolCalls) > 0
}
