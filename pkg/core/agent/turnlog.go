package agent

import (
	"fmt"
	"strings"

	"github.com/riipandi/elph/pkg/ai/provider"
)

// TurnLogFunc writes a tagged diagnostic line (e.g. to the session requests log).
type TurnLogFunc func(kind, text string)

func logProvider(fn TurnLogFunc, kind, text string) {
	if fn == nil || strings.TrimSpace(text) == "" {
		return
	}
	fn(kind, text)
}

func logProviderRequest(fn TurnLogFunc, step int, model string, toolCount, messageCount int, thinking provider.ThinkingConfig) {
	if fn == nil {
		return
	}
	thinkingState := "off"
	if thinking.Enabled {
		thinkingState = string(thinking.ThinkingFormat)
		if thinkingState == "" {
			thinkingState = "enabled"
		}
		if thinking.ReasoningEffort != "" {
			thinkingState += ":" + thinking.ReasoningEffort
		}
	}
	logProvider(fn, "provider_start", fmt.Sprintf(
		"step=%d model=%s tools=%d messages=%d thinking=%s",
		step, model, toolCount, messageCount, thinkingState,
	))
}

func logProviderCancel(fn TurnLogFunc, step int, err error) {
	if fn == nil || err == nil {
		return
	}
	logProvider(fn, "provider_cancel", fmt.Sprintf("step=%d reason=%v", step, err))
}

func logProviderResult(fn TurnLogFunc, step int, result provider.TurnResult, err error) {
	if fn == nil {
		return
	}
	if err != nil {
		logProvider(fn, "provider_error", fmt.Sprintf("step=%d err=%v", step, err))
		return
	}
	logProvider(fn, "provider_ok", fmt.Sprintf(
		"step=%d thinking_len=%d content_len=%d tool_calls=%d tokens_in=%d tokens_out=%d",
		step,
		len(strings.TrimSpace(result.Thinking)),
		len(strings.TrimSpace(result.Content)),
		len(result.ToolCalls),
		result.Usage.InputTokens,
		result.Usage.OutputTokens,
	))
}

func logToolStart(fn TurnLogFunc, step int, call provider.ToolCall) {
	if fn == nil {
		return
	}
	logProvider(fn, "tool_start", fmt.Sprintf("step=%d name=%s id=%s", step, call.Name, call.ID))
}

func logToolDone(fn TurnLogFunc, step int, call provider.ToolCall, result ToolRunResult) {
	if fn == nil {
		return
	}
	errText := ""
	if result.Err != nil {
		errText = result.Err.Error()
	}
	logProvider(fn, "tool_done", fmt.Sprintf(
		"step=%d name=%s id=%s cancelled=%t err=%q out_len=%d",
		step, call.Name, call.ID, result.Cancelled, errText, len(result.Output),
	))
}

func wrapThinkingStream(_ TurnLogFunc, onThinking func(string)) func(string) {
	// Thinking deltas are not logged per chunk — they would block the provider
	// stream on disk I/O. Use provider_ok.thinking_len and the session [thinking]
	// log entry after the turn completes.
	return onThinking
}