package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/stretchr/testify/require"
)

func testModelWithLayout(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)
	return m
}

func TestContentClickDisablesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(mouseClick(1, 1, tea.MouseLeft, 0))
	m = updated.(Model)

	require.False(t, m.mouseEnabled)
	require.True(t, m.selectingText)
}

func TestInputClickKeepsCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(mouseClick(1, m.content.Height()+1, tea.MouseLeft, 0))
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestWheelAfterSelectionResumesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(mouseWheel(1, 1, tea.MouseWheelDown))
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestShiftClickDisablesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(mouseClick(1, m.content.Height()+1, tea.MouseLeft, tea.ModShift))
	m = updated.(Model)

	require.False(t, m.mouseEnabled)
	require.True(t, m.selectingText)
}

func TestResumeMouseAfterSelection(t *testing.T) {
	m := New()
	m.mouseEnabled = false
	m.selectingText = true

	updated, cmd := m.resumeMouseAfterSelection()
	m = updated

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
	require.Nil(t, cmd)
}

func TestResumeMouseWhenAlreadyEnabled(t *testing.T) {
	m := New()
	m.mouseEnabled = true
	m.selectingText = false

	updated, cmd := m.resumeMouseAfterSelection()
	require.True(t, updated.mouseEnabled)
	require.False(t, updated.selectingText)
	require.Nil(t, cmd)
}

func TestKeyAfterSelectionResumesMouse(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(keyRune('a'))
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestContentMouseReleaseUpdatesViewport(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(mouseRelease(1, 1, tea.MouseLeft))
	m = updated.(Model)
	require.NotNil(t, updated)
}
