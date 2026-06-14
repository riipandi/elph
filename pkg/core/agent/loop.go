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
	messages := prepareTurnMessages(opts)

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
			stream.OnThinking = wrapThinkingStream(opts.LogProvider, func(chunk string) {
				sendEvent(ctx, ch, ThinkingDeltaEvent(chunk))
			})
		}

		logProviderRequest(opts.LogProvider, step, opts.Model, len(tools), len(messages), opts.Thinking)

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
			logProviderCancel(opts.LogProvider, step, ctx.Err())
			return
		}
		logProviderResult(opts.LogProvider, step, result, err)
		if err != nil {
			sendEvent(ctx, ch, TurnDoneProviderErrorEvent(err, CompactMessages(messages)))
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
			sendEvent(ctx, ch, TurnDoneWithHistoryEvent(finalResult, CompactMessages(messages)))
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
			logToolStart(opts.LogProvider, step, call)

			runResult := runToolCall(ctx, opts, call)
			logToolDone(opts.LogProvider, step, call, runResult)
			displayResult := LimitToolRunResult(runResult, MaxDisplayToolBytes)
			if !sendEvent(ctx, ch, ToolCallDoneEvent(call, displayResult)) {
				return
			}

			messages = append(messages, provider.ChatMessage{
				Role:       "tool",
				ToolCallID: call.ID,
				Content:    ToolResultMessage(runResult),
			})
		}
		messages = CompactMessages(messages)
	}

	sendEvent(ctx, ch, TurnDoneWithHistoryEvent(provider.TurnResult{
		Content: fmt.Sprintf("Stopped after %d tool rounds.", maxToolIterations),
		Usage:   usage,
	}, CompactMessages(messages)))
}

func runToolCall(ctx context.Context, opts TurnOptions, call provider.ToolCall) ToolRunResult {
	args, err := ParseToolArguments(call.Arguments)
	if err != nil {
		return ToolRunResult{Err: err}
	}

	if kind, needs := ToolInteractKindFor(call.Name, opts.SkipToolApproval); needs {
		if opts.InteractTool == nil {
			return ToolRunResult{Err: fmt.Errorf("tool %s requires user interaction", call.Name)}
		}
		resp, err := opts.InteractTool(ctx, ToolInteractRequest{
			Kind:     kind,
			ToolCall: call,
			Name:     call.Name,
			Args:     args,
		})
		if err != nil {
			return ToolRunResult{Err: err}
		}
		if resp.Cancelled {
			return ToolRunResult{Cancelled: true, Output: "User cancelled"}
		}
		switch kind {
		case ToolInteractAskUser:
			if strings.TrimSpace(resp.Answer) == "" {
				return ToolRunResult{Output: "(no answer)"}
			}
			return ToolRunResult{Output: resp.Answer}
		case ToolInteractApproval:
			if !resp.Approved {
				return ToolRunResult{Output: "User denied tool execution"}
			}
		}
	}

	if opts.ExecuteTool == nil {
		return ToolRunResult{Err: fmt.Errorf("tool executor not configured")}
	}
	return opts.ExecuteTool(ctx, call.Name, args)
}

func mergeUsage(total, delta provider.TurnUsage) provider.TurnUsage {
	total.InputTokens += delta.InputTokens
	total.OutputTokens += delta.OutputTokens
	return total
}
