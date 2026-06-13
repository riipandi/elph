package constants

import "github.com/charmbracelet/lipgloss"

// ─── UI Colors ───────────────────────────────────────────────────────────────

var (
	Blue     = lipgloss.Color("#3B82F6")
	Yellow   = lipgloss.Color("#EAB308")
	Red      = lipgloss.Color("#EF4444")
	Orange   = lipgloss.Color("#F97316")
	Green    = lipgloss.Color("#22C55E")
	Cyan     = lipgloss.Color("#06B6D4")
	Gray     = lipgloss.Color("#6B7280")
	White    = lipgloss.Color("#FFFFFF")
	Purple   = lipgloss.Color("#874BFD")
	PurpleDk = lipgloss.Color("#7C56DC")
	GreenLt  = lipgloss.Color("#43BF6D")
	GreenDk  = lipgloss.Color("#73F59F")
	Violet   = lipgloss.Color("#A78BFA")
	VioletDk = lipgloss.Color("#7C56DC")
)

// ─── Adaptive Colors ─────────────────────────────────────────────────────────

var (
	DimText     = lipgloss.AdaptiveColor{Light: "#9B9B9B", Dark: "#5C5C5C"}
	BrightText  = lipgloss.AdaptiveColor{Light: "#6B7280", Dark: "#D1D5DB"}
	UserPipeCol = lipgloss.AdaptiveColor{Light: "#7C56DC", Dark: "#A78BFA"}
	AIPipeCol   = lipgloss.AdaptiveColor{Light: "#6B7280", Dark: "#9CA3AF"}
)

// ─── Thinking Level Colors ──────────────────────────────────────────────────

func ThinkingColor(lvl ThinkingLevel) lipgloss.Color {
	switch lvl {
	case ThinkingOff:
		return Gray
	case ThinkingMinimal:
		return Cyan
	case ThinkingLow:
		return Green
	case ThinkingMedium:
		return Yellow
	case ThinkingHigh:
		return Orange
	case ThinkingXHigh:
		return Red
	default:
		return Gray
	}
}

// ─── Mode Colors ─────────────────────────────────────────────────────────────

func ModeBorderColor(m AgentMode) lipgloss.Color {
	switch m {
	case ModeBrave:
		return Red
	case ModePlan:
		return Cyan
	case ModeAsk:
		return Blue
	default:
		return Gray
	}
}

// ─── Context Usage Colors ────────────────────────────────────────────────────

func ContextUsageColor(pct float64) lipgloss.Color {
	switch {
	case pct <= 0.50:
		return White
	case pct <= 0.79:
		return Yellow
	case pct <= 0.89:
		return Orange
	default:
		return Red
	}
}
