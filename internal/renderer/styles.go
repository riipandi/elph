package renderer

import (
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
)

// ─── Colors ──────────────────────────────────────────────────────────────────

var (
	// Detect terminal background
	isDarkBackground = lipgloss.HasDarkBackground()

	// Adaptive colors based on terminal background
	blueCol     = lipgloss.Color("#3B82F6")
	yellowCol   = lipgloss.Color("#EAB308")
	highlight   = lipgloss.AdaptiveColor{Light: "#874BFD", Dark: "#7C56DC"}
	special     = lipgloss.AdaptiveColor{Light: "#43BF6D", Dark: "#73F59F"}
	dimText     = lipgloss.AdaptiveColor{Light: "#9B9B9B", Dark: "#5C5C5C"}
	brightText  = lipgloss.AdaptiveColor{Light: "#6B7280", Dark: "#D1D5DB"}
	userPipeCol = lipgloss.AdaptiveColor{Light: "#7C56DC", Dark: "#A78BFA"}
	aiPipeCol   = lipgloss.AdaptiveColor{Light: "#6B7280", Dark: "#9CA3AF"}
	whiteCol    = lipgloss.Color("#FFFFFF")
)

// ─── Mode Border Color ───────────────────────────────────────────────────────

func modeBorderColor(m constants.AgentMode) lipgloss.Color {
	switch m {
	case constants.ModeBrave:
		return lipgloss.Color("#EF4444") // red
	case constants.ModePlan:
		return lipgloss.Color("#06B6D4") // cyan
	case constants.ModeAsk:
		return blueCol // blue
	default:
		return lipgloss.Color("#6B7280") // neutral gray
	}
}

// ContextUsageColor returns color based on context usage percentage.
// <=50% white, <=79% yellow, <=89% orange, >=90% red.
func ContextUsageColor(pct float64) lipgloss.Color {
	switch {
	case pct <= 0.50:
		return whiteCol
	case pct <= 0.79:
		return yellowCol
	case pct <= 0.89:
		return lipgloss.Color("#F97316") // orange
	default:
		return lipgloss.Color("#EF4444") // red
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
