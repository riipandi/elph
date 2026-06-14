package provider

import (
	"encoding/json"
	"strings"
)

type openAIMessage struct {
	Role             string              `json:"role"`
	Content          string              `json:"content"`
	ReasoningContent string              `json:"reasoning_content"`
	ToolCalls        []openAIToolCallRef `json:"tool_calls"`
}

type openAIToolCallRef struct {
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

type openAIChoice struct {
	Message      openAIMessage `json:"message"`
	FinishReason string        `json:"finish_reason"`
}

func parseOpenAIChoice(choice openAIChoice) TurnResult {
	result := TurnResult{
		Thinking: strings.TrimSpace(choice.Message.ReasoningContent),
		Content:  strings.TrimSpace(choice.Message.Content),
	}
	for _, call := range choice.Message.ToolCalls {
		if strings.TrimSpace(call.ID) == "" || strings.TrimSpace(call.Function.Name) == "" {
			continue
		}
		args := json.RawMessage(call.Function.Arguments)
		if len(args) == 0 {
			args = json.RawMessage("{}")
		}
		result.ToolCalls = append(result.ToolCalls, ToolCall{
			ID:        call.ID,
			Name:      call.Function.Name,
			Arguments: args,
		})
	}
	switch choice.FinishReason {
	case "tool_calls":
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

func openAIResultValid(result TurnResult) bool {
	return result.Thinking != "" || result.Content != "" || len(result.ToolCalls) > 0
}

type openAIStreamToolCall struct {
	Index    int    `json:"index"`
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

type openAIStreamToolAccumulator struct {
	calls map[int]*ToolCall
}

func newOpenAIStreamToolAccumulator() *openAIStreamToolAccumulator {
	return &openAIStreamToolAccumulator{calls: make(map[int]*ToolCall)}
}

func (a *openAIStreamToolAccumulator) absorb(delta []openAIStreamToolCall) {
	for _, item := range delta {
		call := a.calls[item.Index]
		if call == nil {
			call = &ToolCall{Arguments: json.RawMessage("{}")}
			a.calls[item.Index] = call
		}
		if item.ID != "" {
			call.ID = item.ID
		}
		if item.Function.Name != "" {
			call.Name = item.Function.Name
		}
		if item.Function.Arguments != "" {
			call.Arguments = appendJSONFragment(call.Arguments, item.Function.Arguments)
		}
	}
}

func (a *openAIStreamToolAccumulator) result() []ToolCall {
	if len(a.calls) == 0 {
		return nil
	}
	max := -1
	for idx := range a.calls {
		if idx > max {
			max = idx
		}
	}
	out := make([]ToolCall, 0, len(a.calls))
	for i := 0; i <= max; i++ {
		if call := a.calls[i]; call != nil && call.Name != "" {
			if len(call.Arguments) == 0 {
				call.Arguments = json.RawMessage("{}")
			}
			out = append(out, *call)
		}
	}
	return out
}

func appendJSONFragment(existing json.RawMessage, fragment string) json.RawMessage {
	if len(existing) == 0 || string(existing) == "{}" {
		return json.RawMessage(fragment)
	}
	return json.RawMessage(string(existing) + fragment)
}
