package renderer

import (
	"github.com/charmbracelet/bubbletea"
)

// Render starts the TUI application using Bubble Tea.
func Render() error {
	m := New()
	// Alt screen keeps redraws stable (no scrollback ghosting). Mouse capture
	// is enabled for viewport wheel scrolling; hold Shift while dragging to
	// select text (mouse capture is released temporarily).
	p := tea.NewProgram(m, tea.WithAltScreen())
	_, err := p.Run()
	return err
}