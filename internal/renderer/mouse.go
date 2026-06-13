package renderer

import (
	"time"

	"github.com/charmbracelet/bubbletea"
)

const mouseReenableDelay = 2 * time.Second

// mouseReenableMsg re-enables mouse capture after a temporary selection pause.
type mouseReenableMsg struct{}

func (m Model) isInContentArea(y int) bool {
	if !m.ready || m.content.Height <= 0 {
		return false
	}
	return y >= 0 && y < m.content.Height
}

func (m Model) shouldReleaseMouseForSelection(evt tea.MouseEvent) bool {
	if evt.IsWheel() || !m.mouseEnabled {
		return false
	}
	if evt.Action != tea.MouseActionPress || evt.Button != tea.MouseButtonLeft {
		return false
	}
	// Left-click in the scrollable content area, or Shift+click anywhere.
	return m.isInContentArea(evt.Y) || evt.Shift
}

func (m Model) beginTextSelection() (Model, []tea.Cmd) {
	m.mouseEnabled = false
	m.selectingText = true
	return m, []tea.Cmd{
		tea.DisableMouse,
		tea.Tick(mouseReenableDelay, func(time.Time) tea.Msg { return mouseReenableMsg{} }),
	}
}

func (m Model) handleMouse(msg tea.MouseMsg) (Model, []tea.Cmd) {
	evt := tea.MouseEvent(msg)

	if m.selectingText {
		return m, nil
	}

	if m.shouldReleaseMouseForSelection(evt) {
		return m.beginTextSelection()
	}

	return m, nil
}

func (m Model) resumeMouseAfterSelection() (Model, tea.Cmd) {
	if m.mouseEnabled && !m.selectingText {
		return m, nil
	}
	m.mouseEnabled = true
	m.selectingText = false
	return m, tea.EnableMouseCellMotion
}