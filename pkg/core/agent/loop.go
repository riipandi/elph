package agent

import (
	"context"
	"fmt"
	"strings"

	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/riipandi/elph/pkg/tools"
)

// DefaultMaxToolIterations is the default max autonomous tool rounds per turn.
const DefaultMaxToolIterations = 25

// maxToolIterationsFor returns the effective tool round limit from opts or default.
func maxToolIterationsFor(opts TurnOptions) int {
	if opts.MaxToolIterations > 0 {
		return opts.MaxToolIterations
	}
	return DefaultMaxToolIterations
}

// toolRoundCountsTowardLimit reports whether a provider tool round should consume
// iteration budget. AskUser-only rounds are excluded because they wait on the
// user rather than autonomous agent work.
func toolRoundCountsTowardLimit(calls []protocol.ToolCall) bool {
	if len(calls) == 0 {
		return false
	}
	for _, call := range calls {
		name, ok := tools.ResolveName(call.Name)
		if !ok || name != tools.AskUser {
			return true
		}
	}
	return false
}

func runProviderLoop(ctx context.Context, opts TurnOptions, ch chan<- Event) {
	messages := prepareTurnMessages(opts)

	providerTools := tools.FilterProviderTools(opts.Tools)
	if len(providerTools) == 0 && opts.ToolsEnabled {
		providerTools = tools.ProviderDefinitions()
	}

	if !sendEvent(ctx, ch, ActivityEvent(ActivityConnecting)) {
		return
	}
	if !sendEvent(ctx, ch, ActivityEvent(ActivityThinking)) {
		return
	}

	var (
		finalResult protocol.TurnResult
		usage       protocol.TurnUsage
	)

	maxIter := maxToolIterationsFor(opts)
	for step := 0; step < maxIter; {
		thinking := opts.Thinking
		showThinking := opts.ShowThinking
		if step > 0 {
			// Tool-result follow-ups (e.g. after deny) should answer quickly without
			// another full reasoning pass.
			if !sendEvent(ctx, ch, ActivityEvent(ActivityThinking)) {
				return
			}
			thinking = protocol.ThinkingConfig{}
			showThinking = false
		}

		stream := &protocol.TurnStream{
			OnContent: func(chunk string) {
				sendEvent(ctx, ch, ResponseDeltaEvent(chunk))
			},
		}
		if showThinking {
			stream.OnThinking = wrapThinkingStream(opts.LogProvider, func(chunk string) {
				sendEvent(ctx, ch, ThinkingDeltaEvent(chunk))
			})
		}

		logProviderRequest(opts.LogProvider, step, opts.Model, len(providerTools), len(messages), thinking)

		result, err := completeProviderWithRetry(ctx, opts.LogProvider, step, opts.Provider, protocol.TurnRequest{
			SystemPrompt: opts.SystemPrompt,
			UserPrompt:   opts.UserPrompt,
			Model:        opts.Model,
			Thinking:     thinking,
			Compat:       opts.Compat,
			Stream:       stream,
			Messages:     messages,
			Tools:        providerTools,
		}, opts.ProviderRetryConfig(), func(attempt int) {
			sendEvent(ctx, ch, ActivityEvent(ActivityConnecting))
		})
		if ctx.Err() != nil {
			logProviderCancel(opts.LogProvider, step, ctx.Err())
			return
		}
		logProviderResult(opts.LogProvider, step, result, err)
		if err != nil {
			if ProviderCancelError(err) {
				logProviderCancel(opts.LogProvider, step, err)
				return
			}
			sendEvent(ctx, ch, TurnDoneProviderErrorEvent(err, CompactMessages(messages)))
			return
		}

		usage = mergeUsage(usage, result.Usage)
		if len(result.ToolCalls) == 0 {
			finalResult = result
			finalResult.Usage = usage
			if !showThinking {
				finalResult.Thinking = ""
			}
			if strings.TrimSpace(result.Content) != "" {
				messages = append(messages, protocol.ChatMessage{
					Role:    "assistant",
					Content: result.Content,
				})
			}
			sendEvent(ctx, ch, TurnDoneWithHistoryEvent(finalResult, CompactMessages(messages)))
			return
		}

		messages = append(messages, protocol.ChatMessage{
			Role:      "assistant",
			Content:   result.Content,
			ToolCalls: append([]protocol.ToolCall(nil), result.ToolCalls...),
		})

		for _, call := range result.ToolCalls {
			if !sendEvent(ctx, ch, ActivityEvent(ActivityForTool(call.Name))) {
				return
			}
			if !sendEvent(ctx, ch, ToolCallStartEvent(call)) {
				return
			}
			logToolStart(opts.LogProvider, step, call)

			runResult := runToolCall(ctx, opts, ch, call)
			logToolDone(opts.LogProvider, step, call, runResult)
			displayResult := LimitToolRunResult(runResult, MaxDisplayToolBytes)
			if !sendEvent(ctx, ch, ToolCallDoneEvent(call, displayResult)) {
				return
			}

			messages = append(messages, protocol.ChatMessage{
				Role:       "tool",
				ToolCallID: call.ID,
				Content:    ToolResultMessage(runResult),
			})
		}
		messages = CompactMessages(messages)
		if toolRoundCountsTowardLimit(result.ToolCalls) {
			step++
		}
	}

	sendEvent(ctx, ch, TurnDoneWithHistoryEvent(protocol.TurnResult{
		Content: fmt.Sprintf("Stopped after %d tool rounds.", maxToolIterationsFor(opts)),
		Usage:   usage,
	}, CompactMessages(messages)))
}

func runToolCall(ctx context.Context, opts TurnOptions, ch chan<- Event, call protocol.ToolCall) ToolRunResult {
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
				return ToolRunResult{Output: ToolDeniedMessage}
			}
		}
	}

	if opts.ExecuteToolStream != nil {
		return opts.ExecuteToolStream(ctx, call, args, func(chunk string) {
			if chunk != "" {
				sendEvent(ctx, ch, ToolCallOutputDeltaEvent(call, chunk))
			}
		})
	}
	if opts.ExecuteTool == nil {
		return ToolRunResult{Err: fmt.Errorf("tool executor not configured")}
	}
	return opts.ExecuteTool(ctx, call.Name, args)
}

func mergeUsage(total, delta protocol.TurnUsage) protocol.TurnUsage {
	total.InputTokens += delta.InputTokens
	total.OutputTokens += delta.OutputTokens
	return total
}
