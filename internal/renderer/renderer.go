package renderer

import "github.com/charmbracelet/bubbletea"

// Render starts the TUI application using Bubble Tea.
func Render() error {
	m := New()
	p := tea.NewProgram(m, tea.WithAltScreen(), tea.WithMouseCellMotion())
	_, err := p.Run()
	return err
}
