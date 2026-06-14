package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestToolInteractOfferShowsAskUserDialog(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.width = 100
	m.ready = true
	m = m.beginAgentTurn()
	bridge := newToolInteractBridge()
	m.agent.ToolInteractBridge = bridge

	offer := toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Args: map[string]any{"question": "Pick one"},
		},
		RespCh: make(chan agent.ToolInteractResponse, 1),
	}
	bridge.inbox <- offer

	updated, _ := m.Update(toolInteractOfferMsg{offer: offer})
	m = updated.(Model)

	require.True(t, m.toolInteractDialogActive())
	view := stripANSI(m.inputChromeView())
	require.Contains(t, view, "AskUser")
	require.Contains(t, view, "Pick one")
	require.False(t, m.input.Focused())
}

func TestToolInteractOfferMsgReturnsWithoutFallingThrough(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.width = 100
	m.ready = true
	m.messages = []message{{text: "hi", kind: constants.MessageUser}}

	offer := toolInteractOffer{
		Req: agent.ToolInteractRequest{
			Kind: agent.ToolInteractAskUser,
			Args: map[string]any{"question": "Q"},
		},
		RespCh: make(chan agent.ToolInteractResponse, 1),
	}

	updated, cmd := m.Update(toolInteractOfferMsg{offer: offer})
	m = updated.(Model)
	require.True(t, m.toolInteractDialogActive())
	require.NotNil(t, cmd)
}