package renderer

import (
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
)

// ─── Colors ──────────────────────────────────────────────────────────────────

var (
	blueCol   = lipgloss.Color("#3B82F6")
	highlight = lipgloss.AdaptiveColor{Light: "#874BFD", Dark: "#7C56DC"}
	special   = lipgloss.AdaptiveColor{Light: "#43BF6D", Dark: "#73F59F"}
	dimText   = lipgloss.AdaptiveColor{Light: "#9B9B9B", Dark: "#5C5C5C"}
)

// ─── Mode Border Color ───────────────────────────────────────────────────────

func modeBorderColor(m constants.AgentMode) lipgloss.Color {
	switch m {
	case constants.ModeBrave:
		return lipgloss.Color("#EF4444")
	case constants.ModePlan:
		return lipgloss.Color("#06B6D4")
	case constants.ModeAsk:
		return lipgloss.Color("#22C55E")
	default:
		return lipgloss.Color("#A855F7")
	}
}

// ─── Style Builders ──────────────────────────────────────────────────────────

func bannerStyle(w int) lipgloss.Style {
	return lipgloss.NewStyle().
		Width(w-2).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(blueCol).
		Padding(1, 2)
}

func inputStyle(w int, m constants.AgentMode) lipgloss.Style {
	return lipgloss.NewStyle().
		Width(w-2).
		Border(lipgloss.RoundedBorder()).
		BorderForeground(modeBorderColor(m)).
		Padding(0, 1)
}

func footerStyle(w int) lipgloss.Style {
	return lipgloss.NewStyle().
		Width(w).
		Padding(0, 1)
}
