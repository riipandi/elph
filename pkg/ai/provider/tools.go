package provider

import (
	"encoding/json"
	"strings"
)

// BuildMessages returns provider messages, using explicit history when present.
func BuildMessages(req TurnRequest) []ChatMessage {
	if len(req.Messages) > 0 {
		return append([]ChatMessage(nil), req.Messages...)
	}
	out := make([]ChatMessage, 0, 1)
	if strings.TrimSpace(req.UserPrompt) != "" {
		out = append(out, ChatMessage{Role: "user", Content: req.UserPrompt})
	}
	return out
}

// OpenAITools converts tool definitions to OpenAI chat completions format.
func OpenAITools(tools []ToolDefinition) []map[string]any {
	if len(tools) == 0 {
		return nil
	}
	out := make([]map[string]any, 0, len(tools))
	for _, tool := range tools {
		out = append(out, map[string]any{
			"type": "function",
			"function": map[string]any{
				"name":        tool.Name,
				"description": tool.Description,
				"parameters":  tool.Parameters,
			},
		})
	}
	return out
}

// AnthropicTools converts tool definitions to Anthropic Messages API format.
func AnthropicTools(tools []ToolDefinition) []map[string]any {
	if len(tools) == 0 {
		return nil
	}
	out := make([]map[string]any, 0, len(tools))
	for _, tool := range tools {
		out = append(out, map[string]any{
			"name":         tool.Name,
			"description":  tool.Description,
			"input_schema": tool.Parameters,
		})
	}
	return out
}

// OpenAIMessages converts chat messages to OpenAI request message objects.
func OpenAIMessages(systemPrompt string, messages []ChatMessage, thinking ThinkingConfig, compat Compat) []map[string]any {
	capacity := len(messages)
	if strings.TrimSpace(systemPrompt) != "" {
		capacity++
	}
	out := make([]map[string]any, 0, capacity)
	if strings.TrimSpace(systemPrompt) != "" {
		role := "system"
		if thinking.Enabled && compat.supportsDeveloperRole() {
			role = "developer"
		}
		out = append(out, map[string]any{"role": role, "content": systemPrompt})
	}
	for _, msg := range messages {
		switch msg.Role {
		case "assistant":
			entry := map[string]any{"role": "assistant"}
			if strings.TrimSpace(msg.Content) != "" {
				entry["content"] = msg.Content
			}
			if len(msg.ToolCalls) > 0 {
				entry["tool_calls"] = openAIToolCallsPayload(msg.ToolCalls)
			}
			out = append(out, entry)
		case "tool":
			out = append(out, map[string]any{
				"role":         "tool",
				"tool_call_id": msg.ToolCallID,
				"content":      msg.Content,
			})
		default:
			out = append(out, map[string]any{"role": msg.Role, "content": msg.Content})
		}
	}
	return out
}

func openAIToolCallsPayload(calls []ToolCall) []map[string]any {
	out := make([]map[string]any, 0, len(calls))
	for _, call := range calls {
		out = append(out, map[string]any{
			"id":   call.ID,
			"type": "function",
			"function": map[string]string{
				"name":      call.Name,
				"arguments": string(call.Arguments),
			},
		})
	}
	return out
}

// AnthropicMessages converts chat messages to Anthropic request format.
func AnthropicMessages(messages []ChatMessage) []map[string]any {
	out := make([]map[string]any, 0, len(messages))
	for _, msg := range messages {
		switch msg.Role {
		case "assistant":
			blocks := make([]map[string]any, 0, 1+len(msg.ToolCalls))
			if strings.TrimSpace(msg.Content) != "" {
				blocks = append(blocks, map[string]any{"type": "text", "text": msg.Content})
			}
			for _, call := range msg.ToolCalls {
				var input map[string]any
				_ = json.Unmarshal(call.Arguments, &input)
				if input == nil {
					input = map[string]any{}
				}
				blocks = append(blocks, map[string]any{
					"type":  "tool_use",
					"id":    call.ID,
					"name":  call.Name,
					"input": input,
				})
			}
			out = append(out, map[string]any{"role": "assistant", "content": blocks})
		case "tool":
			out = append(out, map[string]any{
				"role": "user",
				"content": []map[string]any{{
					"type":        "tool_result",
					"tool_use_id": msg.ToolCallID,
					"content":     msg.Content,
				}},
			})
		default:
			out = append(out, map[string]any{"role": msg.Role, "content": msg.Content})
		}
	}
	return out
}
