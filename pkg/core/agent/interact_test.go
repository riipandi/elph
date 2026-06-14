package agent

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestToolInteractKindFor(t *testing.T) {
	kind, ok := ToolInteractKindFor(tool.AskUser, false)
	require.True(t, ok)
	require.Equal(t, ToolInteractAskUser, kind)

	kind, ok = ToolInteractKindFor(tool.Bash, false)
	require.True(t, ok)
	require.Equal(t, ToolInteractApproval, kind)

	_, ok = ToolInteractKindFor(tool.Bash, true)
	require.False(t, ok)

	_, ok = ToolInteractKindFor(tool.Read, false)
	require.False(t, ok)
}

func TestRunTurnAskUserInteract(t *testing.T) {
	stub := &loopStubProvider{steps: []provider.TurnResult{
		{
			StopReason: provider.StopReasonToolUse,
			ToolCalls: []provider.ToolCall{{
				ID:        "call_ask",
				Name:      "AskUser",
				Arguments: json.RawMessage(`{"question":"Pick one","options":["a","b"]}`),
			}},
		},
		{Content: "Thanks.", StopReason: provider.StopReasonEndTurn},
	}}

	events := RunTurn(context.Background(), TurnOptions{
		UserPrompt:   "help me choose",
		Provider:     stub,
		ToolsEnabled: true,
		InteractTool: func(ctx context.Context, req ToolInteractRequest) (ToolInteractResponse, error) {
			require.Equal(t, ToolInteractAskUser, req.Kind)
			require.Equal(t, "Pick one", req.Args["question"])
			return ToolInteractResponse{Answer: "a"}, nil
		},
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			t.Fatal("AskUser should not call ExecuteTool")
			return ToolRunResult{}
		},
	})

	var done Event
	for evt := range events {
		if evt.Kind == EventToolCallDone {
			require.Equal(t, "a", evt.ToolResult.Output)
		}
		if evt.Kind == EventTurnDone {
			done = evt
		}
	}
	require.Equal(t, "Thanks.", done.Response)
}

func TestRunTurnBashApprovalDenied(t *testing.T) {
	stub := &loopStubProvider{steps: []provider.TurnResult{
		{
			StopReason: provider.StopReasonToolUse,
			ToolCalls: []provider.ToolCall{{
				ID:        "call_bash",
				Name:      "Bash",
				Arguments: json.RawMessage(`{"command":"echo no"}`),
			}},
		},
		{Content: "OK", StopReason: provider.StopReasonEndTurn},
	}}

	events := RunTurn(context.Background(), TurnOptions{
		UserPrompt:   "run",
		Provider:     stub,
		ToolsEnabled: true,
		InteractTool: func(ctx context.Context, req ToolInteractRequest) (ToolInteractResponse, error) {
			require.Equal(t, ToolInteractApproval, req.Kind)
			return ToolInteractResponse{Approved: false}, nil
		},
		ExecuteTool: func(ctx context.Context, name string, args map[string]any) ToolRunResult {
			t.Fatal("denied bash should not execute")
			return ToolRunResult{}
		},
	})

	for evt := range events {
		if evt.Kind == EventToolCallDone {
			require.Equal(t, "User denied tool execution", evt.ToolResult.Output)
		}
	}
}
