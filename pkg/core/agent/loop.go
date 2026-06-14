package agent

import (
	"context"
	"fmt"
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool"
)

const maxToolIterations = 8

func runProviderLoop(ctx context.Context, opts TurnOptions, ch chan<- Event) {
	messages := append([]provider.ChatMessage(nil), opts.Messages...)
	if len(messages) == 0 && strings.TrimSpace(opts.UserPrompt) != "" {
		messages = append(messages, provider.ChatMessage{Role: "user", Content: opts.UserPrompt})
	}

	tools := tool.FilterProviderTools(opts.Tools)
	if len(tools) == 0 && opts.ToolsEnabled {
		tools = tool.ProviderDefinitions()
	}

	if !sendEvent(ctx, ch, ActivityEvent(ActivityConnecting)) {
		return
	}
	if !sendEvent(ctx, ch, ActivityEvent(ActivityThinking)) {
		return
	}

	var (
		finalResult provider.TurnResult
		usage       provider.TurnUsage
	)

	for step := 0; step < maxToolIterations; step++ {
		stream := &provider.TurnStream{
			OnContent: func(chunk string) {
				sendEvent(ctx, ch, ResponseDeltaEvent(chunk))
			},
		}
		if opts.ShowThinking {
			stream.OnThinking = func(chunk string) {
				sendEvent(ctx, ch, ThinkingDeltaEvent(chunk))
			}
		}

		result, err := opts.Provider.Complete(ctx, provider.TurnRequest{
			SystemPrompt: opts.SystemPrompt,
			UserPrompt:   opts.UserPrompt,
			Model:        opts.Model,
			Thinking:     opts.Thinking,
			Compat:       opts.Compat,
			Stream:       stream,
			Messages:     messages,
			Tools:        tools,
		})
		if ctx.Err() != nil {
			return
		}
		if err != nil {
			sendEvent(ctx, ch, TurnDoneWithHistoryEvent(provider.TurnResult{
				Content: fmt.Sprintf("Provider error: %v", err),
			}, messages))
			return
		}

		usage = mergeUsage(usage, result.Usage)
		if len(result.ToolCalls) == 0 {
			finalResult = result
			finalResult.Usage = usage
			if !opts.ShowThinking {
				finalResult.Thinking = ""
			}
			if strings.TrimSpace(result.Content) != "" {
				messages = append(messages, provider.ChatMessage{
					Role:    "assistant",
					Content: result.Content,
				})
			}
			sendEvent(ctx, ch, TurnDoneWithHistoryEvent(finalResult, messages))
			return
		}

		messages = append(messages, provider.ChatMessage{
			Role:      "assistant",
			Content:   result.Content,
			ToolCalls: append([]provider.ToolCall(nil), result.ToolCalls...),
		})

		for _, call := range result.ToolCalls {
			if !sendEvent(ctx, ch, ActivityEvent(ActivityForTool(call.Name))) {
				return
			}
			if !sendEvent(ctx, ch, ToolCallStartEvent(call)) {
				return
			}

			runResult := runToolCall(ctx, opts, call)
			if !sendEvent(ctx, ch, ToolCallDoneEvent(call, runResult)) {
				return
			}

			messages = append(messages, provider.ChatMessage{
				Role:       "tool",
				ToolCallID: call.ID,
				Content:    ToolResultMessage(runResult),
			})
		}
	}

	sendEvent(ctx, ch, TurnDoneWithHistoryEvent(provider.TurnResult{
		Content: fmt.Sprintf("Stopped after %d tool rounds.", maxToolIterations),
		Usage:   usage,
	}, messages))
}

func runToolCall(ctx context.Context, opts TurnOptions, call provider.ToolCall) ToolRunResult {
	if opts.ExecuteTool == nil {
		return ToolRunResult{Err: fmt.Errorf("tool executor not configured")}
	}
	args, err := ParseToolArguments(call.Arguments)
	if err != nil {
		return ToolRunResult{Err: err}
	}
	return opts.ExecuteTool(ctx, call.Name, args)
}

func mergeUsage(total, delta provider.TurnUsage) provider.TurnUsage {
	total.InputTokens += delta.InputTokens
	total.OutputTokens += delta.OutputTokens
	return total
}
