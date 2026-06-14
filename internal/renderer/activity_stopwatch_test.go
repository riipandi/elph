package renderer

import (
	"testing"
	"time"

	"charm.land/bubbles/v2/stopwatch"
	"github.com/stretchr/testify/require"
)

func TestFormatCompactElapsed(t *testing.T) {
	require.Equal(t, "0ms", formatCompactElapsed(0))
	require.Equal(t, "450ms", formatCompactElapsed(450*time.Millisecond))
	require.Equal(t, "4.2s", formatCompactElapsed(4200*time.Millisecond))
	require.Equal(t, "12s", formatCompactElapsed(12*time.Second))
	require.Equal(t, "1m05s", formatCompactElapsed(65*time.Second))
}

func TestActivityViewShowsElapsedTime(t *testing.T) {
	m := testInputModel(t)
	m.width = 100
	m = m.beginAgentTurn()

	for _, msg := range drainTeaCmd(m.activityStopwatchStartCmd()) {
		m.agent.Stopwatch, _ = m.agent.Stopwatch.Update(msg)
	}
	m.agent.Stopwatch, _ = m.agent.Stopwatch.Update(stopwatch.TickMsg{ID: m.agent.Stopwatch.ID()})

	view := stripANSI(m.activityView())
	require.Contains(t, view, "Connecting")
	require.Contains(t, view, "Esc to cancel")
	require.Regexp(t, `\d+ms|\d+\.\d+s|\d+s`, view)
}

func TestStopActivityStopwatchHaltsElapsedUpdates(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()
	for _, msg := range drainTeaCmd(m.activityStopwatchStartCmd()) {
		m.agent.Stopwatch, _ = m.agent.Stopwatch.Update(msg)
	}
	m.agent.Stopwatch, _ = m.agent.Stopwatch.Update(stopwatch.TickMsg{ID: m.agent.Stopwatch.ID()})
	elapsed := m.agent.Stopwatch.Elapsed()
	require.Greater(t, elapsed, time.Duration(0))

	m = m.stopActivityStopwatch()
	require.False(t, m.agent.Stopwatch.Running())
	require.Equal(t, elapsed, m.agent.Stopwatch.Elapsed())
}
