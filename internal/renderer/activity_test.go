package renderer

import (
	"testing"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestActivityViewHiddenWhenIdle(t *testing.T) {
	m := New()
	m.width = 80
	require.Empty(t, m.activityView())
}

func TestActivityViewShowsLabel(t *testing.T) {
	m := New()
	m.width = 80
	m.activity = constants.ActivityWriting
	m.spinnerFrame = 0

	view := m.activityView()
	require.Contains(t, view, "Writing")
	require.Equal(t, 2, lipgloss.Height(view), "activity view should be 2 lines (margin + label)")
}

func TestSubmitStartsAgentActivity(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)

	m.input.SetValue("hello")
	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
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

func TestBeginAgentTurnIncreasesChrome(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	idle := m.syncLayout(false)
	idleChrome := idle.chromeH
	idleVP := idle.content.Height

	busy := idle.beginAgentTurn().syncLayout(true)

	require.Greater(t, busy.chromeH, idleChrome)
	require.Less(t, busy.content.Height, idleVP)
}

func TestAgentPhaseDelaysAreOrdered(t *testing.T) {
	require.Positive(t, agentPhaseDelay)
	require.Less(t, spinnerInterval, agentPhaseDelay)
	require.GreaterOrEqual(t, len(constants.AgentTurnPhases), 2)
}