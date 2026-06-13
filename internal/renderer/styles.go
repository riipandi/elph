package renderer

import (
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
)

// ─── Style Builders ──────────────────────────────────────────────────────────

func bannerStyle(w int) lipgloss.Style {
	return lipgloss.NewStyle().
		Width(w - 2).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(constants.Blue).
		Padding(1, 2)
}

func inputStyle(w int, m constants.AgentMode) lipgloss.Style {
	return lipgloss.NewStyle().
		Width(w - 2).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(constants.ModeBorderColor(m)).
		Padding(0, 1)
}
