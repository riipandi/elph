package renderer

import (
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestActivityViewHiddenWhenIdle(t *testing.T) {
	m := New()
	m.width = 80
	require.Empty(t, m.activityView())
}

func TestInputHasTopMarginWhenIdle(t *testing.T) {
	m := testInputModel(t)
	require.Equal(t, agent.ActivityIdle, m.agent.Activity)
	require.Greater(t, lipgloss.Height(m.inputView()), lipgloss.Height(m.inputBodyView())+1)

	m = m.beginAgentTurn()
	require.GreaterOrEqual(t, lipgloss.Height(m.inputView()), lipgloss.Height(m.inputBodyView()))
}

func TestActivityViewShowsLabel(t *testing.T) {
	m := New()
	m.width = 80
	m.agent.Busy = true
	m.agent.Activity = agent.ActivityWriting
	m.agent.SpinnerFrame = 0

	view := m.activityView()
	require.Contains(t, view, "Writing")
	require.Equal(t, 1, lipgloss.Height(view), "activity view should be 1 line")
}

func TestInputStaysFocusedDuringAgentTurn(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")
	updated, _ := m.Update(keyEnter())
	m = updated.(Model)

	require.True(t, m.agent.Busy)
	require.True(t, m.input.Focused())
}

func TestSubmitStartsAgentActivity(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)

	m.input.SetValue("hello")
	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.NotNil(t, cmd)
	require.True(t, m.agent.Busy)
	require.Equal(t, agent.ActivityConnecting, m.agent.Activity)
	require.NotEmpty(t, m.activityView())
}

func TestActivityProgression(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.beginAgentTurn()

	updated, _ := m.Update(agentEventMsg{event: agent.ActivityEvent(agent.ActivityReading)})
	m = updated.(Model)
	require.Equal(t, agent.ActivityReading, m.agent.Activity)

	updated, _ = m.Update(agentEventMsg{event: agent.TurnDoneEvent("done")})
	m = updated.(Model)
	require.False(t, m.agent.Busy)
	require.Equal(t, agent.ActivityIdle, m.agent.Activity)
	require.Len(t, m.messages, 1)
	require.Equal(t, constants.MessageAI, m.messages[0].kind)
}

func TestBeginAgentTurnSwapsInputMarginForActivity(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	idle := m.syncLayout(false)
	idleChrome := idle.layout.ChromeH
	idleVP := idle.content.Height()

	busy := idle.beginAgentTurn().syncLayout(true)

	require.NotEmpty(t, busy.activityView())
	require.Equal(t, idleChrome, busy.layout.ChromeH, "activity line replaces idle input top margin")
	require.Equal(t, idleVP, busy.content.Height())
}

func TestAgentPhaseDelaysAreOrdered(t *testing.T) {
	require.Positive(t, agent.PhaseDelay)
	require.Less(t, spinnerInterval, agent.PhaseDelay)
	require.GreaterOrEqual(t, len(agent.TurnPhases), 2)
}
