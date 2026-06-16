package openai

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
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
	// Sanitize: remove orphaned tool messages whose tool_call_id doesn't match
	// a preceding assistant's tool_calls. DeepSeek and other OpenAI-compatible
	// APIs reject tool messages without a matching parent.
	messages = sanitizeToolMessages(messages)

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
				args := string(provider.NormalizeToolArguments(call.Arguments))
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
			out = append(out, userMessageParam(msg))
		}
	}
	return out
}

// sanitizeToolMessages removes tool messages whose tool_call_id doesn't match
// any preceding assistant message's tool_calls. This prevents provider API
// errors from orphaned tool messages (e.g. after history compaction).
func sanitizeToolMessages(messages []provider.ChatMessage) []provider.ChatMessage {
	var activeIDs []string
	out := make([]provider.ChatMessage, 0, len(messages))
	for _, msg := range messages {
		if msg.Role == "assistant" {
			// Collect IDs from this assistant's tool_calls.
			activeIDs = nil
			for _, call := range msg.ToolCalls {
				if call.ID != "" {
					activeIDs = append(activeIDs, call.ID)
				}
			}
			out = append(out, msg)
		} else if msg.Role == "tool" {
			// Only include tool messages whose ID matches a preceding assistant's tool_calls.
			if msg.ToolCallID == "" {
				continue
			}
			var matched bool
			for _, id := range activeIDs {
				if id == msg.ToolCallID {
					matched = true
					break
				}
			}
			if matched {
				out = append(out, msg)
			}
		} else {
			// User messages reset the active IDs (new turn).
			activeIDs = nil
			out = append(out, msg)
		}
	}
	return out
}

func userMessageParam(msg provider.ChatMessage) openaisdk.ChatCompletionMessageParamUnion {
	if len(msg.Images) == 0 {
		return openaisdk.UserMessage(msg.Content)
	}
	parts := make([]openaisdk.ChatCompletionContentPartUnionParam, 0, 1+len(msg.Images))
	if trimmed := strings.TrimSpace(msg.Content); trimmed != "" {
		parts = append(parts, openaisdk.TextContentPart(trimmed))
	}
	for _, img := range msg.Images {
		if len(img.Data) == 0 {
			continue
		}
		mime := strings.TrimSpace(img.MIME)
		if mime == "" {
			mime = "image/png"
		}
		url := fmt.Sprintf("data:%s;base64,%s", mime, base64.StdEncoding.EncodeToString(img.Data))
		parts = append(parts, openaisdk.ImageContentPart(openaisdk.ChatCompletionContentPartImageImageURLParam{
			URL: url,
		}))
	}
	if len(parts) == 0 {
		return openaisdk.UserMessage(msg.Content)
	}
	return openaisdk.ChatCompletionMessageParamUnion{
		OfUser: &openaisdk.ChatCompletionUserMessageParam{
			Content: openaisdk.ChatCompletionUserMessageParamContentUnion{
				OfArrayOfContentParts: parts,
			},
		},
	}
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
		result.ToolCalls = append(result.ToolCalls, provider.ToolCall{
			ID:        fn.ID,
			Name:      fn.Function.Name,
			Arguments: provider.NormalizeToolArguments(json.RawMessage(fn.Function.Arguments)),
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
	for _, key := range []string{
		"reasoning_content",
		"reasoning",
		"reasoning_details",
		"thinking",
		"thought",
		"reasoning_text",
	} {
		if text := extraFieldString(extra, key); text != "" {
			return text
		}
	}
	var vendor struct {
		ReasoningContent string          `json:"reasoning_content"`
		Reasoning        json.RawMessage `json:"reasoning"`
		ReasoningDetails json.RawMessage `json:"reasoning_details"`
		Thinking         string          `json:"thinking"`
		Thought          string          `json:"thought"`
		ReasoningText    string          `json:"reasoning_text"`
	}
	if err := json.Unmarshal([]byte(rawJSON), &vendor); err != nil {
		return ""
	}
	if vendor.ReasoningContent != "" {
		return vendor.ReasoningContent
	}
	if vendor.Thinking != "" {
		return vendor.Thinking
	}
	if len(vendor.Reasoning) > 0 {
		if text := decodeJSONString(string(vendor.Reasoning)); text != "" {
			return text
		}
	}
	if len(vendor.ReasoningDetails) > 0 {
		if text := decodeJSONString(string(vendor.ReasoningDetails)); text != "" {
			return text
		}
	}
	if vendor.Thought != "" {
		return vendor.Thought
	}
	return vendor.ReasoningText
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
	var obj map[string]json.RawMessage
	if err := json.Unmarshal([]byte(raw), &obj); err == nil {
		for _, key := range []string{"content", "text", "summary", "reasoning"} {
			part, ok := obj[key]
			if !ok {
				continue
			}
			if text := decodeJSONString(string(part)); text != "" {
				return text
			}
		}
		return ""
	}
	var arr []map[string]json.RawMessage
	if err := json.Unmarshal([]byte(raw), &arr); err == nil {
		var b strings.Builder
		for _, item := range arr {
			for _, key := range []string{"text", "content", "summary"} {
				part, ok := item[key]
				if !ok {
					continue
				}
				if text := decodeJSONString(string(part)); text != "" {
					b.WriteString(text)
				}
			}
		}
		return b.String()
	}
	return raw
}

type streamToolAccumulator struct {
	calls map[int]*provider.ToolCall
}

func newStreamToolAccumulator() *streamToolAccumulator {
	return &streamToolAccumulator{calls: make(map[int]*provider.ToolCall)}
}

func (a *streamToolAccumulator) absorbJSON(index int, id, name, args string) {
	call := a.calls[index]
	if call == nil {
		call = &provider.ToolCall{Arguments: json.RawMessage("{}")}
		a.calls[index] = call
	}
	if id != "" {
		call.ID = id
	}
	if name != "" {
		call.Name = name
	}
	if args != "" {
		call.Arguments = appendJSONFragment(call.Arguments, args)
	}
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
			call.Arguments = provider.NormalizeToolArguments(call.Arguments)
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
