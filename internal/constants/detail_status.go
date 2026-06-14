package constants

import (
	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
)

// DetailStatus drives detail-box background and foreground colors.
// Dark palette aligned with pi interactive theme (toolPending/Success/Error backgrounds).
type DetailStatus int

const (
	DetailStatusNeutral DetailStatus = iota
	DetailStatusRunning
	DetailStatusSuccess
	DetailStatusWarning
	DetailStatusError
)

type detailStatusColors struct {
	accent compat.AdaptiveColor
	bodyFg compat.AdaptiveColor
	bg     compat.AdaptiveColor
}

func detailPalette(status DetailStatus) detailStatusColors {
	switch status {
	case DetailStatusRunning:
		return detailStatusColors{
			accent: compat.AdaptiveColor{Light: lipgloss.Color("#547DA7"), Dark: lipgloss.Color("#5F87FF")},
			bodyFg: compat.AdaptiveColor{Light: lipgloss.Color("#6C6C6C"), Dark: lipgloss.Color("#808080")},
			bg:     compat.AdaptiveColor{Light: lipgloss.Color("#E8E8F0"), Dark: lipgloss.Color("#282832")},
		}
	case DetailStatusSuccess:
		return detailStatusColors{
			accent: compat.AdaptiveColor{Light: lipgloss.Color("#588458"), Dark: lipgloss.Color("#B5BD68")},
			bodyFg: compat.AdaptiveColor{Light: lipgloss.Color("#6C6C6C"), Dark: lipgloss.Color("#808080")},
			bg:     compat.AdaptiveColor{Light: lipgloss.Color("#E8F0E8"), Dark: lipgloss.Color("#283228")},
		}
	case DetailStatusWarning:
		return detailStatusColors{
			accent: compat.AdaptiveColor{Light: lipgloss.Color("#9A7326"), Dark: lipgloss.Color("#B8B86A")},
			bodyFg: compat.AdaptiveColor{Light: lipgloss.Color("#6C6C6C"), Dark: lipgloss.Color("#808080")},
			bg:     compat.AdaptiveColor{Light: lipgloss.Color("#FFF8E8"), Dark: lipgloss.Color("#3C3728")},
		}
	case DetailStatusError:
		return detailStatusColors{
			accent: compat.AdaptiveColor{Light: lipgloss.Color("#AA5555"), Dark: lipgloss.Color("#CC6666")},
			bodyFg: compat.AdaptiveColor{Light: lipgloss.Color("#6C6C6C"), Dark: lipgloss.Color("#808080")},
			bg:     compat.AdaptiveColor{Light: lipgloss.Color("#F0E8E8"), Dark: lipgloss.Color("#3C2828")},
		}
	default:
		return detailStatusColors{
			accent: compat.AdaptiveColor{Light: lipgloss.Color("#5A8080"), Dark: lipgloss.Color("#8ABEB7")},
			bodyFg: compat.AdaptiveColor{Light: lipgloss.Color("#6C6C6C"), Dark: lipgloss.Color("#808080")},
			bg:     compat.AdaptiveColor{Light: lipgloss.Color("#F3F2F6"), Dark: lipgloss.Color("#2D2838")},
		}
	}
}

// DetailStatusStyle returns the lipgloss style for a detail block body.
func DetailStatusStyle(status DetailStatus) lipgloss.Style {
	p := detailPalette(status)
	return lipgloss.NewStyle().Foreground(p.bodyFg).Background(p.bg)
}

// DetailStatusAccent returns a foreground style for detail chevrons.
func DetailStatusAccent(status DetailStatus) lipgloss.Style {
	p := detailPalette(status)
	return lipgloss.NewStyle().Foreground(p.accent)
}

// DetailStatusBodyStyle returns foreground-only style for detail body text.
func DetailStatusBodyStyle(status DetailStatus) lipgloss.Style {
	p := detailPalette(status)
	return lipgloss.NewStyle().Foreground(p.bodyFg)
}

// DetailStatusPreviewLabel returns the collapsed preview label for an active status.
func DetailStatusPreviewLabel(status DetailStatus) string {
	switch status {
	case DetailStatusRunning:
		return "Running..."
	default:
		return ""
	}
}
