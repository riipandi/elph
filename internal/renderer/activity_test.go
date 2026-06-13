package renderer

import (
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestActivityViewHiddenWhenIdle(t *testing.T) {
	m := New()
	m.width = 80
	require.Empty(t, m.activityView())
}

func TestInputHasTopMarginWhenIdle(t *testing.T) {
	m := testInputModel(t)
	require.Equal(t, constants.ActivityIdle, m.activity)
	require.Greater(t, lipgloss.Height(m.inputView()), lipgloss.Height(m.inputBodyView())+1)

	m = m.beginAgentTurn()
	require.GreaterOrEqual(t, lipgloss.Height(m.inputView()), lipgloss.Height(m.inputBodyView()))
}

func TestActivityViewShowsLabel(t *testing.T) {
	m := New()
	m.width = 80
	m.activity = constants.ActivityWriting
	m.spinnerFrame = 0

	view := m.activityView()
	require.Contains(t, view, "Writing")
	require.Equal(t, 1, lipgloss.Height(view), "activity view should be 1 line")
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
	require.True(t, m.busy)
	require.Equal(t, constants.ActivityConnecting, m.activity)
	require.NotEmpty(t, m.activityView())
}

func TestActivityProgression(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.beginAgentTurn()

	updated, _ := m.Update(ActivityMsg{Activity: constants.ActivityReading})
	m = updated.(Model)
	require.Equal(t, constants.ActivityReading, m.activity)

	updated, _ = m.Update(AgentDoneMsg{Response: "done"})
	m = updated.(Model)
	require.False(t, m.busy)
	require.Equal(t, constants.ActivityIdle, m.activity)
	require.Len(t, m.messages, 1)
	require.Equal(t, constants.MessageAI, m.messages[0].kind)
}

func TestBeginAgentTurnSwapsInputMarginForActivity(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	idle := m.syncLayout(false)
	idleChrome := idle.chromeH
	idleVP := idle.content.Height()

	busy := idle.beginAgentTurn().syncLayout(true)

	require.NotEmpty(t, busy.activityView())
	require.Equal(t, idleChrome, busy.chromeH, "activity line replaces idle input top margin")
	require.Equal(t, idleVP, busy.content.Height())
}

func TestAgentPhaseDelaysAreOrdered(t *testing.T) {
	require.Positive(t, agentPhaseDelay)
	require.Less(t, spinnerInterval, agentPhaseDelay)
	require.GreaterOrEqual(t, len(constants.AgentTurnPhases), 2)
}
