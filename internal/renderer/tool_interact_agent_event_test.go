package renderer

import (
	"encoding/json"
	"testing"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestToolCallStartProcessedWhileApprovalDialogOpen(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.width = 100
	m.ready = true
	m = m.beginAgentTurn()

	offer := toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractApproval,
			Name: "Bash",
			Args: map[string]any{"command": "echo hi"},
		},
		RespCh: make(chan agent.ToolInteractResponse, 1),
	}
	updated, _ := m.Update(toolInteractOfferMsg{Offer: offer})
	m = updated.(Model)
	require.True(t, m.toolInteractDialogActive())

	call := provider.ToolCall{
		ID:        "call_echo",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"echo hi"}`),
	}
	updated, cmd := m.Update(agentEventMsg{event: agent.ToolCallStartEvent(call)})
	m = updated.(Model)
	require.NotNil(t, cmd)
	require.True(t, m.toolInteractDialogActive(), "dialog stays open while tool starts")

	idx := m.agent.NativeToolMsgIDs["call_echo"]
	require.Contains(t, m.messages[idx].text, "(running...)")
	require.Equal(t, uiconst.DetailStatusRunning, m.messages[idx].detailStatus)

	view := stripANSI(m.contentView())
	require.Contains(t, view, "Bash(echo hi)")
	require.Contains(t, view, "(running...)")
	require.NotContains(t, view, "Running...")
}

func TestResponseDeltaProcessedWhileApprovalDialogOpen(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.width = 100
	m.ready = true
	m = m.beginAgentTurn()

	offer := toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractApproval,
			Name: "Bash",
		},
		RespCh: make(chan agent.ToolInteractResponse, 1),
	}
	updated, _ := m.Update(toolInteractOfferMsg{Offer: offer})
	m = updated.(Model)

	updated, cmd := m.Update(agentEventMsg{event: agent.ResponseDeltaEvent("partial")})
	m = updated.(Model)
	require.NotNil(t, cmd)
	require.GreaterOrEqual(t, m.agent.ResponseMsgID, 0)
	require.Contains(t, m.messages[m.agent.ResponseMsgID].text, "partial")
}
