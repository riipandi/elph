package constants

import (
	"image/color"

	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
)

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
	DimText    = compat.AdaptiveColor{Light: lipgloss.Color("#9B9B9B"), Dark: lipgloss.Color("#5C5C5C")}
	BrightText = compat.AdaptiveColor{Light: lipgloss.Color("#6B7280"), Dark: lipgloss.Color("#D1D5DB")}
	Highlight  = compat.AdaptiveColor{Light: lipgloss.Color("#874BFD"), Dark: lipgloss.Color("#7C56DC")}
)

// ─── Thinking Level Colors ──────────────────────────────────────────────────

func ThinkingColor(lvl ThinkingLevel) color.Color {
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

func ModeBorderColor(m AgentMode) color.Color {
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

func ContextUsageColor(pct float64) color.Color {
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
