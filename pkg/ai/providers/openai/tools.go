package openai

import (
	"encoding/json"
	"strings"

	openaisdk "github.com/openai/openai-go/v3"
	"github.com/openai/openai-go/v3/packages/respjson"
	"github.com/openai/openai-go/v3/shared"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

func chatTools(tools []provider.ToolDefinition) []openaisdk.ChatCompletionToolUnionParam {
	if len(tools) == 0 {
		return nil
	}
	out := make([]openaisdk.ChatCompletionToolUnionParam, 0, len(tools))
	for _, tool := range tools {
		params := shared.FunctionParameters{}
		for key, value := range tool.Parameters {
			params[key] = value
		}
		out = append(out, openaisdk.ChatCompletionFunctionTool(shared.FunctionDefinitionParam{
			Name:        tool.Name,
			Description: openaisdk.String(tool.Description),
			Parameters:  params,
		}))
	}
	return out
}

func chatMessages(systemPrompt string, messages []provider.ChatMessage, thinking provider.ThinkingConfig, compat provider.Compat) []openaisdk.ChatCompletionMessageParamUnion {
	capacity := len(messages)
	if strings.TrimSpace(systemPrompt) != "" {
		capacity++
	}
	out := make([]openaisdk.ChatCompletionMessageParamUnion, 0, capacity)
	if strings.TrimSpace(systemPrompt) != "" {
		if thinking.Enabled && compat.DeveloperRoleSupported() {
			out = append(out, openaisdk.DeveloperMessage(systemPrompt))
		} else {
			out = append(out, openaisdk.SystemMessage(systemPrompt))
		}
	}
	for _, msg := range messages {
		switch msg.Role {
		case "assistant":
			asst := openaisdk.ChatCompletionAssistantMessageParam{}
			if strings.TrimSpace(msg.Content) != "" {
				asst.Content = openaisdk.ChatCompletionAssistantMessageParamContentUnion{
					OfString: openaisdk.String(msg.Content),
				}
			}
			for _, call := range msg.ToolCalls {
				args := string(call.Arguments)
				if args == "" {
					args = "{}"
				}
				asst.ToolCalls = append(asst.ToolCalls, openaisdk.ChatCompletionMessageToolCallUnionParam{
					OfFunction: &openaisdk.ChatCompletionMessageFunctionToolCallParam{
						ID: call.ID,
						Function: openaisdk.ChatCompletionMessageFunctionToolCallFunctionParam{
							Name:      call.Name,
							Arguments: args,
						},
					},
				})
			}
			out = append(out, openaisdk.ChatCompletionMessageParamUnion{OfAssistant: &asst})
		case "tool":
			out = append(out, openaisdk.ToolMessage(msg.Content, msg.ToolCallID))
		default:
			out = append(out, openaisdk.UserMessage(msg.Content))
		}
	}
	return out
}

func turnResultFromChatChoice(choice openaisdk.ChatCompletionChoice, hooks Hooks) provider.TurnResult {
	message := choice.Message
	reasoning := hooks.ChoiceReasoning
	if reasoning == nil {
		reasoning = choiceReasoningText
	}
	result := provider.TurnResult{
		Thinking: strings.TrimSpace(reasoning(choice)),
		Content:  strings.TrimSpace(message.Content),
	}
	for _, call := range message.ToolCalls {
		fn := call.AsFunction()
		if strings.TrimSpace(fn.ID) == "" || strings.TrimSpace(fn.Function.Name) == "" {
			continue
		}
		args := json.RawMessage(fn.Function.Arguments)
		if len(args) == 0 {
			args = json.RawMessage("{}")
		}
		result.ToolCalls = append(result.ToolCalls, provider.ToolCall{
			ID:        fn.ID,
			Name:      fn.Function.Name,
			Arguments: args,
		})
	}
	switch choice.FinishReason {
	case "tool_calls":
		result.StopReason = provider.StopReasonToolUse
	default:
		if len(result.ToolCalls) > 0 {
			result.StopReason = provider.StopReasonToolUse
		} else {
			result.StopReason = provider.StopReasonEndTurn
		}
	}
	return result
}

func resultValid(result provider.TurnResult) bool {
	return result.Thinking != "" || result.Content != "" || len(result.ToolCalls) > 0
}

func choiceReasoningText(choice openaisdk.ChatCompletionChoice) string {
	message := choice.Message
	return reasoningText(message.JSON.ExtraFields, message.RawJSON())
}

func streamReasoningText(delta openaisdk.ChatCompletionChunkChoiceDelta) string {
	return reasoningText(delta.JSON.ExtraFields, delta.RawJSON())
}

func reasoningText(extra map[string]respjson.Field, rawJSON string) string {
	if text := extraFieldString(extra, "reasoning_content"); text != "" {
		return text
	}
	if text := extraFieldString(extra, "reasoning"); text != "" {
		return text
	}
	var vendor struct {
		ReasoningContent string `json:"reasoning_content"`
		Reasoning        string `json:"reasoning"`
	}
	if err := json.Unmarshal([]byte(rawJSON), &vendor); err != nil {
		return ""
	}
	if vendor.ReasoningContent != "" {
		return vendor.ReasoningContent
	}
	return vendor.Reasoning
}

func extraFieldString(fields map[string]respjson.Field, key string) string {
	if len(fields) == 0 {
		return ""
	}
	field, ok := fields[key]
	if !ok || !field.Valid() {
		return ""
	}
	return decodeJSONString(field.Raw())
}

func decodeJSONString(raw string) string {
	if raw == "" || raw == "null" {
		return ""
	}
	var value string
	if err := json.Unmarshal([]byte(raw), &value); err == nil {
		return value
	}
	return raw
}

type streamToolAccumulator struct {
	calls map[int]*provider.ToolCall
}

func newStreamToolAccumulator() *streamToolAccumulator {
	return &streamToolAccumulator{calls: make(map[int]*provider.ToolCall)}
}

func (a *streamToolAccumulator) absorbSDK(delta []openaisdk.ChatCompletionChunkChoiceDeltaToolCall) {
	for _, item := range delta {
		idx := int(item.Index)
		call := a.calls[idx]
		if call == nil {
			call = &provider.ToolCall{Arguments: json.RawMessage("{}")}
			a.calls[idx] = call
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

func (a *streamToolAccumulator) result() []provider.ToolCall {
	if len(a.calls) == 0 {
		return nil
	}
	max := -1
	for idx := range a.calls {
		if idx > max {
			max = idx
		}
	}
	out := make([]provider.ToolCall, 0, len(a.calls))
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

func turnUsageFromCompletion(usage openaisdk.CompletionUsage) provider.TurnUsage {
	return provider.TurnUsage{
		InputTokens:  int(usage.PromptTokens),
		OutputTokens: int(usage.CompletionTokens),
	}
}
