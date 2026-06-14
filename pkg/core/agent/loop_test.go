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
