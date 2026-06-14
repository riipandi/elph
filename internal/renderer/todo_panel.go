package renderer

import (
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/memz"
)

var (
	todoPanelBorder = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(constants.DimText).
			Padding(0, 1)
	todoPanelTitleStyle = lipgloss.NewStyle().Foreground(constants.BrightText).Bold(true)
	todoDoneStyle       = lipgloss.NewStyle().Foreground(constants.Green)
	todoActiveStyle     = lipgloss.NewStyle().Foreground(constants.Yellow).Bold(true)
	todoPendingStyle    = lipgloss.NewStyle().Foreground(constants.DimText)
)

func (m Model) showsTodoPanel() bool {
	return len(m.session.Todos) > 0
}

func (m Model) todoPanelHeight() int {
	if !m.showsTodoPanel() {
		return 0
	}
	return lipgloss.Height(m.todoPanelView())
}

func (m Model) todoPanelView() string {
	if !m.showsTodoPanel() {
		return ""
	}

	width := max(m.width, 1)
	innerW := max(width-4, 1)

	var lines []string
	title := "Tasks"
	if m.agent.TodoListUpdating {
		frame := spinnerFrames[m.agent.SpinnerFrame%len(spinnerFrames)]
		title = frame + " " + title
	}
	lines = append(lines, todoPanelTitleStyle.Render(title))

	for _, item := range m.session.Todos {
		lines = append(lines, m.todoPanelLine(item, innerW))
	}

	body := lipgloss.JoinVertical(lipgloss.Left, lines...)
	return lipgloss.NewStyle().
		MarginTop(1).
		Width(width).
		Render(todoPanelBorder.Width(width).Render(body))
}

func (m Model) todoPanelLine(item memz.Todo, maxWidth int) string {
	marker, textStyle := m.todoPanelMarker(item.Status)
	line := marker + " " + item.Title
	if maxWidth > 0 {
		line = ansi.Truncate(line, maxWidth, "…")
	}
	return textStyle.Render(line)
}

func (m Model) todoPanelMarker(status memz.Status) (string, lipgloss.Style) {
	switch status {
	case memz.StatusDone:
		return "✓", todoDoneStyle
	case memz.StatusInProgress:
		if m.agent.Busy || m.agent.TodoListUpdating {
			frame := spinnerFrames[m.agent.SpinnerFrame%len(spinnerFrames)]
			return frame, todoActiveStyle
		}
		return "◐", todoActiveStyle
	default:
		return "○", todoPendingStyle
	}
}
