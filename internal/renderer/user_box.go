package renderer

import (
	"strings"

	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/compat"
	"github.com/riipandi/elph/internal/constants"
)

const userLeftBarWidth = 1

func userBoxInnerWidth(blockWidth, hPad int) int {
	return max(blockWidth-2*hPad-userLeftBarWidth, 1)
}

func userBoxContentWidth(blockWidth int) int {
	return max(blockWidth-userLeftBarWidth, 1)
}

func renderUserLeftBarColumn(lineCount int, bg, accent compat.AdaptiveColor) string {
	if lineCount < 1 {
		lineCount = 1
	}
	style := constants.UserLeftBarStyle(bg, accent).Width(userLeftBarWidth).MaxHeight(1)
	lines := make([]string, lineCount)
	for i := range lines {
		lines[i] = style.Render("▎")
	}
	return strings.Join(lines, "\n")
}

func renderUserBoxWithLeftBar(blockWidth int, bg, accent compat.AdaptiveColor, box lipgloss.Style, vPad, hPad int, content string) string {
	inner := box.Padding(vPad, hPad).Width(userBoxContentWidth(blockWidth)).Render(content)
	bar := renderUserLeftBarColumn(lipgloss.Height(inner), bg, accent)
	return lipgloss.JoinHorizontal(lipgloss.Top, bar, inner)
}
