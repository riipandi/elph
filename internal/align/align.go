package align

import (
	"strings"

	"charm.land/lipgloss/v2"
)

const ColumnGap = 2

// ColumnWidth returns the display width of the widest value.
func ColumnWidth(values ...string) int {
	width := 0
	for _, value := range values {
		if w := lipgloss.Width(value); w > width {
			width = w
		}
	}
	return width
}

// Row splits a name and summary into aligned columns.
func Row(name string, nameColW int, summary string) (string, string, string) {
	gap := strings.Repeat(" ", max(nameColW-lipgloss.Width(name)+ColumnGap, ColumnGap))
	return name, gap, summary
}