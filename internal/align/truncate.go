package align

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// TruncateDisplayWidth truncates s to at most maxW display cells wide.
// If maxW is less than 1, returns an empty string.
func TruncateDisplayWidth(s string, maxW int) string {
	if maxW < 1 {
		return ""
	}
	if lipgloss.Width(s) <= maxW {
		return s
	}

	// Iterate rune-by-rune, accumulating display width.
	var b strings.Builder
	w := 0
	for _, r := range s {
		rw := lipgloss.Width(string(r))
		if w+rw > maxW {
			break
		}
		b.WriteRune(r)
		w += rw
	}
	return b.String()
}
