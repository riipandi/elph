package renderer

import (
	"time"

	tea "charm.land/bubbletea/v2"
)

const mouseReenableDelay = 2 * time.Second

// mouseReenableMsg re-enables mouse capture after a temporary selection pause.
type mouseReenableMsg struct{}

func (m Model) isInContentArea(y int) bool {
	if !m.ready || m.content.Height() <= 0 {
		return false
	}
	return y >= 0 && y < m.content.Height()
}

func (m Model) shouldReleaseMouseForSelection(msg tea.MouseMsg) bool {
	if !m.mouseEnabled {
		return false
	}
	click, ok := msg.(tea.MouseClickMsg)
	if !ok {
		return false
	}
	if click.Button != tea.MouseLeft {
		return false
	}
	// Left-click in the scrollable content area, or Shift+click anywhere.
	return m.isInContentArea(click.Y) || click.Mod.Contains(tea.ModShift)
}

func (m Model) beginTextSelection() (Model, []tea.Cmd) {
	m.mouseEnabled = false
	m.selectingText = true
	return m, []tea.Cmd{
		tea.Tick(mouseReenableDelay, func(time.Time) tea.Msg { return mouseReenableMsg{} }),
	}
}

func (m Model) handleMouse(msg tea.MouseMsg) (Model, []tea.Cmd) {
	if m.selectingText {
		return m, nil
	}

	if m.shouldReleaseMouseForSelection(msg) {
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
	return m, nil
}
