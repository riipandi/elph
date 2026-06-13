package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestInputScrollbarOverlaysLastColumn(t *testing.T) {
	m := testInputModel(t)
	lines := make([]string, maxInputLines+2)
	for i := range lines {
		lines[i] = fmt.Sprintf("line %d", i+1)
	}
	m.input.SetValue(strings.Join(lines, "\n"))
	m = m.syncInputWidth()

	require.True(t, m.inputScrollable())

	body := m.input.View()
	inner := m.inputBodyView()
	require.Equal(t, lipgloss.Width(body), lipgloss.Width(inner))
	require.True(t, strings.Contains(inner, "█") || strings.Contains(inner, "░"),
		"overlay should include scrollbar glyphs")
	sideBySide := lipgloss.JoinHorizontal(lipgloss.Top, body, m.inputScrollBarView())
	require.NotEqual(t, lipgloss.Width(sideBySide), lipgloss.Width(inner),
		"side-by-side layout should be wider than overlay")
}

func TestInputScrollbarSitsFlushToRightPadding(t *testing.T) {
	m := testInputModel(t)
	lines := make([]string, maxInputLines+2)
	for i := range lines {
		lines[i] = fmt.Sprintf("line %d", i+1)
	}
	m.input.SetValue(strings.Join(lines, "\n"))
	m = m.syncInputWidth()

	inner := m.inputBodyView()
	prefixW := 0
	if m.showPromptPrefix {
		prefix := lipgloss.NewStyle().Foreground(constants.White).Bold(true).Render(m.promptChar + " ")
		prefixW = lipgloss.Width(prefix)
	}
	wantInnerW := inputContentWidth(m.chromeOuterWidth()) - prefixW
	require.Equal(t, wantInnerW, lipgloss.Width(inner))

	boxW := borderedChromeWidth(m.chromeOuterWidth())
	border := cachedInputBorder(m.mode)
	rendered := border.Width(boxW).Render(inner)
	require.Equal(t, m.chromeOuterWidth(), lipgloss.Width(rendered))
}

func TestOverlayInputScrollBarShortLine(t *testing.T) {
	body := "hi" + strings.Repeat(" ", 10)
	bar := scrollBarFor(1, 2, 0)
	got := overlayInputScrollBar(body, bar)
	require.Equal(t, lipgloss.Width(body), lipgloss.Width(got))
	plain := stripANSI(got)
	require.True(t, strings.HasSuffix(plain, "░") || strings.HasSuffix(plain, "█"),
		"last column should be scrollbar: %q", plain)
}