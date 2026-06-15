package agent

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

type loopStubProvider struct {
	steps []provider.TurnResult
	calls int
}

func (s *loopStubProvider) ID() string { return "stub" }

func (s *loopStubProvider) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	if s.calls >= len(s.steps) {
		return provider.TurnResult{Content: "done"}, nil
	}
	result := s.steps[s.calls]
	s.calls++
	return result, nil
}

type recordingProvider struct {
	lastMessages []provider.ChatMessage
}

func (r *recordingProvider) ID() string { return "recording" }

func (r *recordingProvider) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	r.lastMessages = append([]provider.ChatMessage(nil), req.Messages...)
	return provider.TurnResult{Content: "ok", StopReason: provider.StopReasonEndTurn}, nil
}

func TestRunTurnAppendsFollowUpPromptToHistory(t *testing.T) {
	stub := &recordingProvider{}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	history := []provider.ChatMessage{
		{Role: "user", Content: "first"},
		{Role: "assistant", Content: "answer one"},
	}

	for evt := range RunTurn(ctx, TurnOptions{
		UserPrompt:   "second",
		Provider:     stub,
		ToolsEnabled: true,
		Messages:     history,
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			return ToolRunResult{Output: "unused"}
		},
	}) {
		if evt.Kind == EventTurnDone {
			break
		}
	}

	require.Len(t, stub.lastMessages, 3)
	require.Equal(t, "second", stub.lastMessages[2].Content)
}

func TestRunTurnDisablesThinkingOnToolFollowUp(t *testing.T) {
	stub := &loopStubProvider{steps: []provider.TurnResult{
		{
			StopReason: provider.StopReasonToolUse,
			ToolCalls: []provider.ToolCall{{
				ID:        "call_1",
				Name:      "Bash",
				Arguments: json.RawMessage(`{"command":"echo hi"}`),
			}},
		},
		{Content: "Done.", StopReason: provider.StopReasonEndTurn},
	}}
	rec := &thinkingRecordingProvider{inner: stub}

	events := RunTurn(context.Background(), TurnOptions{
		UserPrompt:   "run",
		Provider:     rec,
		ToolsEnabled: true,
		Thinking:     provider.ThinkingConfig{Enabled: true, ThinkingFormat: provider.ThinkingFormatQwen},
		ShowThinking: true,
		InteractTool: func(ctx context.Context, req ToolInteractRequest) (ToolInteractResponse, error) {
			return ToolInteractResponse{Approved: false}, nil
		},
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			t.Fatal("denied bash should not execute")
			return ToolRunResult{}
		},
	})

	var afterDeny bool
	var sawFollowUpThinking bool
	for evt := range events {
		if evt.Kind == EventToolCallDone {
			afterDeny = true
		}
		if afterDeny && evt.Kind == EventActivity && evt.Activity == ActivityThinking {
			sawFollowUpThinking = true
		}
	}

	require.True(t, sawFollowUpThinking, "follow-up provider call should show Thinking activity")
	require.Len(t, rec.thinking, 2)
	require.True(t, rec.thinking[0].Enabled)
	require.False(t, rec.thinking[1].Enabled)
}

type thinkingRecordingProvider struct {
	inner    *loopStubProvider
	thinking []provider.ThinkingConfig
}

func (r *thinkingRecordingProvider) ID() string { return r.inner.ID() }

func (r *thinkingRecordingProvider) Complete(ctx context.Context, req provider.TurnRequest) (provider.TurnResult, error) {
	r.thinking = append(r.thinking, req.Thinking)
	return r.inner.Complete(ctx, req)
}

func TestRunTurnAskUserRoundsDoNotCountTowardLimit(t *testing.T) {
	steps := make([]provider.TurnResult, 0, maxToolIterations+2)
	for range maxToolIterations + 1 {
		steps = append(steps, provider.TurnResult{
			StopReason: provider.StopReasonToolUse,
			ToolCalls: []provider.ToolCall{{
				ID:        "call_ask",
				Name:      "AskUser",
				Arguments: json.RawMessage(`{"question":"Pick one","options":["a","b"]}`),
			}},
		})
	}
	steps = append(steps, provider.TurnResult{
		Content:    "Done.",
		StopReason: provider.StopReasonEndTurn,
	})

	stub := &loopStubProvider{steps: steps}
	events := RunTurn(context.Background(), TurnOptions{
		UserPrompt:   "help me choose",
		Provider:     stub,
		ToolsEnabled: true,
		InteractTool: func(ctx context.Context, req ToolInteractRequest) (ToolInteractResponse, error) {
			require.Equal(t, ToolInteractAskUser, req.Kind)
			return ToolInteractResponse{Answer: "a"}, nil
		},
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			t.Fatal("AskUser should not call ExecuteTool")
			return ToolRunResult{}
		},
	})

	var done Event
	for evt := range events {
		if evt.Kind == EventTurnDone {
			done = evt
		}
	}
	require.Equal(t, "Done.", done.Response)
	require.Equal(t, len(steps), stub.calls)
}

func TestRunTurnNativeToolLoop(t *testing.T) {
	stub := &loopStubProvider{steps: []provider.TurnResult{
		{
			StopReason: provider.StopReasonToolUse,
			ToolCalls: []provider.ToolCall{{
				ID:        "call_1",
				Name:      "Read",
				Arguments: json.RawMessage(`{"path":"README.md"}`),
			}},
		},
		{Content: "Found the readme.", StopReason: provider.StopReasonEndTurn},
	}}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	events := RunTurn(ctx, TurnOptions{
		UserPrompt:   "read readme",
		Provider:     stub,
		ToolsEnabled: true,
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			require.Equal(t, "Read", name)
			return ToolRunResult{Output: "hello readme"}
		},
	})

	var (
		toolStarts int
		toolDone   int
		done       Event
	)
	for evt := range events {
		switch evt.Kind {
		case EventToolCallStart:
			toolStarts++
		case EventToolCallDone:
			toolDone++
			require.Equal(t, "hello readme", evt.ToolResult.Output)
		case EventTurnDone:
			done = evt
		}
	}

	require.Equal(t, 1, toolStarts)
	require.Equal(t, 1, toolDone)
	require.Equal(t, "Found the readme.", done.Response)
	require.NotEmpty(t, done.History)
}
