package anthropic

import (
	"encoding/base64"
	"encoding/json"
	"strings"

	"github.com/anthropics/anthropic-sdk-go"
	provider "github.com/riipandi/elph/pkg/ai/protocol"
)

func anthropicTools(tools []provider.ToolDefinition) []anthropic.ToolUnionParam {
	if len(tools) == 0 {
		return nil
	}
	out := make([]anthropic.ToolUnionParam, 0, len(tools))
	for _, tool := range tools {
		variant := anthropic.ToolUnionParamOfTool(anthropicToolInputSchema(tool.Parameters), tool.Name)
		if variant.OfTool != nil && tool.Description != "" {
			variant.OfTool.Description = anthropic.String(tool.Description)
		}
		out = append(out, variant)
	}
	return out
}

func anthropicToolInputSchema(params map[string]any) anthropic.ToolInputSchemaParam {
	schema := anthropic.ToolInputSchemaParam{
		ExtraFields: make(map[string]any),
	}
	if props, ok := params["properties"]; ok {
		schema.Properties = props
	}
	if req, ok := params["required"]; ok {
		switch v := req.(type) {
		case []string:
			schema.Required = v
		case []any:
			for _, item := range v {
				if s, ok := item.(string); ok {
					schema.Required = append(schema.Required, s)
				}
			}
		}
	}
	for key, value := range params {
		switch key {
		case "properties", "required", "type":
			continue
		default:
			schema.ExtraFields[key] = value
		}
	}
	return schema
}

func anthropicMessages(messages []provider.ChatMessage) []anthropic.MessageParam {
	out := make([]anthropic.MessageParam, 0, len(messages))
	for _, msg := range messages {
		switch msg.Role {
		case "assistant":
			blocks := make([]anthropic.ContentBlockParamUnion, 0, 1+len(msg.ToolCalls))
			if strings.TrimSpace(msg.Content) != "" {
				blocks = append(blocks, anthropic.NewTextBlock(msg.Content))
			}
			for _, call := range msg.ToolCalls {
				args := provider.NormalizeToolArguments(call.Arguments)
				var input map[string]any
				_ = json.Unmarshal(args, &input)
				if input == nil {
					input = map[string]any{}
				}
				blocks = append(blocks, anthropic.NewToolUseBlock(call.ID, input, call.Name))
			}
			out = append(out, anthropic.NewAssistantMessage(blocks...))
		case "tool":
			out = append(out, anthropic.NewUserMessage(
				anthropic.NewToolResultBlock(msg.ToolCallID, msg.Content, false),
			))
		default:
			out = append(out, anthropic.NewUserMessage(userContentBlocks(msg)...))
		}
	}
	return out
}

func userContentBlocks(msg provider.ChatMessage) []anthropic.ContentBlockParamUnion {
	blocks := make([]anthropic.ContentBlockParamUnion, 0, 1+len(msg.Images))
	if trimmed := strings.TrimSpace(msg.Content); trimmed != "" {
		blocks = append(blocks, anthropic.NewTextBlock(trimmed))
	}
	for _, img := range msg.Images {
		if len(img.Data) == 0 {
			continue
		}
		mime := strings.TrimSpace(img.MIME)
		if mime == "" {
			mime = "image/png"
		}
		blocks = append(blocks, anthropic.NewImageBlockBase64(mime, base64.StdEncoding.EncodeToString(img.Data)))
	}
	if len(blocks) == 0 {
		return []anthropic.ContentBlockParamUnion{anthropic.NewTextBlock(msg.Content)}
	}
	return blocks
}

func turnResultFromMessage(msg *anthropic.Message) provider.TurnResult {
	var result provider.TurnResult
	for _, block := range msg.Content {
		switch variant := block.AsAny().(type) {
		case anthropic.ThinkingBlock:
			if variant.Thinking != "" {
				if result.Thinking != "" {
					result.Thinking += "\n"
				}
				result.Thinking += variant.Thinking
			}
		case anthropic.TextBlock:
			if variant.Text != "" {
				if result.Content != "" {
					result.Content += "\n"
				}
				result.Content += variant.Text
			}
		case anthropic.ToolUseBlock:
			if variant.ID == "" || variant.Name == "" {
				continue
			}
			result.ToolCalls = append(result.ToolCalls, provider.ToolCall{
				ID:        variant.ID,
				Name:      variant.Name,
				Arguments: provider.NormalizeToolArguments(variant.Input),
			})
		}
	}
	switch msg.StopReason {
	case anthropic.StopReasonToolUse:
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
