package renderer

import "github.com/charmbracelet/bubbletea"

// Render starts the TUI application using Bubble Tea.
func Render() error {
	m := New()
	p := tea.NewProgram(m)
	_, err := p.Run()
	return err
}
