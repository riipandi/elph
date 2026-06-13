package renderer

import (
	"fmt"

	"github.com/charmbracelet/bubbletea"
)

// Render starts the TUI application using Bubble Tea.
func Render() error {
	m := New()
	fmt.Println(m.bannerView())
	p := tea.NewProgram(m)
	_, err := p.Run()
	return err
}
