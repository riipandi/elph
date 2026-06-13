package renderer

import (
	"testing"

	"github.com/charmbracelet/bubbletea"
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

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
	m = updated.(Model)

	require.False(t, m.mouseEnabled)
	require.True(t, m.selectingText)
}

func TestInputClickKeepsCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: m.content.Height + 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestWheelAfterSelectionResumesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonWheelDown,
	})
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}

func TestShiftClickDisablesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: m.content.Height + 1, Shift: true,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
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
	require.NotNil(t, cmd)
}

func TestKeyAfterSelectionResumesMouse(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'a'}})
	m = updated.(Model)

	require.True(t, m.mouseEnabled)
	require.False(t, m.selectingText)
}