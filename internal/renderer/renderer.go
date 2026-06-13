package renderer

import tea "charm.land/bubbletea/v2"

// Render starts the TUI application using Bubble Tea.
func Render() error {
	m := New()
	// Alt screen and mouse mode are declared declaratively in View().
	p := tea.NewProgram(m)
	_, err := p.Run()
	return err
}
