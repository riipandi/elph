package renderer

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestCancelAgentTurnWithEscape(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()

	updated, cancelCmd := m.Update(keyEscape())
	m = updated.(Model)
	require.Nil(t, cancelCmd)
	require.False(t, m.agent.Busy)
	require.Contains(t, stripANSI(m.messages[len(m.messages)-1].text), "agent turn cancelled")
}

func TestAgentActivityShowsCancelHint(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()
	m.width = 100

	view := stripANSI(m.activityView())
	require.Contains(t, view, "Esc to cancel")
}

func TestCtrlCDuringAgentTurnCancelsNotExit(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()

	updated, cmd := m.Update(keyCtrl('c'))
	m = updated.(Model)

	require.False(t, m.quitting)
	require.Nil(t, cmd)
	require.False(t, m.agent.Busy)
	require.Contains(t, stripANSI(m.messages[len(m.messages)-1].text), "agent turn cancelled")
}

func TestAgentTurnClosedOpensPendingMarkupAskUser(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.width = 100
	m.ready = true
	m = m.beginAgentTurn()
	m.agent.MarkupAskUserPending = &markupAskUserOffer{
		Name: "AskUser",
		Parameters: map[string]string{
			"question": "Pick one",
			"options":  `["English", "Indonesia"]`,
		},
	}

	updated, cmd := m.Update(agentTurnClosedMsg{})
	m = updated.(Model)
	require.NotNil(t, cmd)
	require.False(t, m.agent.Busy)

	updated, _ = m.Update(cmd())
	m = updated.(Model)
	require.True(t, m.toolInteractDialogActive())
}

func TestAgentTurnClosedResetsBusyState(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()

	updated, _ := m.Update(agentTurnClosedMsg{})
	m = updated.(Model)
	require.False(t, m.agent.Busy)
}
