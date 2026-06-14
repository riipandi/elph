package theme

import (
	"os"

	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
)

// Mode is the user-facing theme preference.
type Mode string

const (
	Auto  Mode = "auto"
	Dark  Mode = "dark"
	Light Mode = "light"
)

// Parse normalizes a theme preference string. Unknown values default to auto.
func Parse(raw string) Mode {
	switch Mode(raw) {
	case Dark, Light:
		return Mode(raw)
	default:
		return Auto
	}
}

// Resolve picks the effective dark/light appearance.
func Resolve(preference Mode, terminalDark bool) bool {
	switch preference {
	case Dark:
		return true
	case Light:
		return false
	default:
		return terminalDark
	}
}

// DetectTerminal reports whether the terminal background is dark.
func DetectTerminal() bool {
	return lipgloss.HasDarkBackground(os.Stdin, os.Stdout)
}

// Apply sets the global adaptive color mode used by compat.AdaptiveColor.
func Apply(dark bool) {
	compat.HasDarkBackground = dark
}

// IsDark reports the currently applied appearance.
func IsDark() bool {
	return compat.HasDarkBackground
}

// Next cycles auto → dark → light → auto.
func Next(current Mode) Mode {
	switch current {
	case Auto:
		return Dark
	case Dark:
		return Light
	default:
		return Auto
	}
}
