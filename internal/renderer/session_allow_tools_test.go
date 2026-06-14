package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestBuildTurnOptionsSkipsApprovalWhenSessionAllowed(t *testing.T) {
	m := testInputModel(t)
	m.agent.SessionAllowTools = true

	opts := m.buildTurnOptions("run tools", nil, nil)
	require.True(t, opts.SkipToolApproval)
}

func TestSessionAllowPersistsAcrossTurns(t *testing.T) {
	m := testInputModel(t)
	respCh := make(chan agent.ToolInteractResponse, 1)
	m.toolInteractForm = newToolApprovalForm(agent.ToolInteractRequest{
		Kind: agent.ToolInteractApproval,
		Name: "Bash",
	}, 60)
	m.toolInteractPending = toolInteractOffer{
		Req:    agent.ToolInteractRequest{Kind: agent.ToolInteractApproval, Name: "Bash"},
		RespCh: respCh,
	}

	bridge := newToolInteractBridge()
	m.agent.ToolInteractBridge = bridge
	m, _ = m.completeToolInteractWith(agent.ToolInteractResponse{
		Approved:     true,
		AllowSession: true,
	})
	require.True(t, m.agent.SessionAllowTools)
	require.True(t, bridge.skipSessionApproval)
	require.True(t, (<-respCh).AllowSession)

	opts := m.buildTurnOptions("next", nil, newToolInteractBridge())
	require.True(t, opts.SkipToolApproval)
	require.Equal(t, constants.ModeBuild, m.mode)
}
