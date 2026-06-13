package renderer

import (
	"strings"

	"github.com/riipandi/elph/internal/align"
)

type paletteRow struct {
	name    string
	summary string
}

func paletteLine(name, gap, summary string, selected bool) string {
	var nameStyled, summaryStyled string
	if selected {
		nameStyled = cmdPaletteSelected.Render(name)
		summaryStyled = cmdPaletteSummarySelected.Render(summary)
	} else {
		nameStyled = cmdPaletteName.Render(name)
		summaryStyled = dimStyle.Render(summary)
	}
	return nameStyled + gap + summaryStyled
}

func (m Model) paletteBox(lines []string) string {
	if len(lines) == 0 {
		return ""
	}
	inner := strings.Join(lines, "\n")
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	return paletteBorder(m.mode).Width(boxW).Render(inner)
}

func (m Model) renderPaletteRows(rows []paletteRow, selected int, nameColW int) string {
	lines := make([]string, len(rows))
	for i, row := range rows {
		_, gap, summary := align.Row(row.name, nameColW, row.summary)
		lines[i] = paletteLine(row.name, gap, summary, i == selected)
	}
	return m.paletteBox(lines)
}