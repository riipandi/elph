package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/stretchr/testify/require"
)

func TestBannerMetadataLineClamp(t *testing.T) {
	m := New()
	m.width = 50
	m.workDir = strings.Repeat("x", 80)

	banner := m.bannerView()
	require.LessOrEqual(t, lipgloss.Width(banner), m.width)
	require.NotContains(t, banner, strings.Repeat("x", 80))
	require.Contains(t, banner, "Directory:")
}

func TestBannerTipWraps(t *testing.T) {
	m := New()
	m.width = 40
	m.tip = strings.Repeat("word ", 30)

	banner := m.bannerView()
	require.GreaterOrEqual(t, lipgloss.Height(banner), 12)
}

func TestFooterLineClamp(t *testing.T) {
	m := New()
	m.width = 42
	m.modelName = "Claude Sonnet 4.6 Extended Edition"

	footer := m.footerView()
	require.LessOrEqual(t, lipgloss.Width(footer), m.width)

	lines := strings.Split(strings.TrimSpace(footer), "\n")
	require.Len(t, lines, 2)
	require.LessOrEqual(t, lipgloss.Width(lines[0]), footerContentWidth(m.width)+1)
}

func TestFooterRowSpacing(t *testing.T) {
	left := "Claude Sonnet 4.6 | anthropic | T: high | IMG"
	right := "$0.00 | 0.0% (262k)"
	row := footerRow(60, left, right)

	require.NotContains(t, row, "IMG$0.00")
	require.True(t, strings.HasSuffix(row, "(262k)"))
	require.Equal(t, 60, lipgloss.Width(row))
}

func TestFooterRightSegmentFlush(t *testing.T) {
	m := New()
	m.width = 80
	m.workDir = "/Users/dev/renderer"
	m.contextWindow = 262144

	lines := strings.Split(strings.TrimSpace(m.footerView()), "\n")
	require.Len(t, lines, 2)

	maxLineW := footerContentWidth(m.width) + 1
	for i, line := range lines {
		require.LessOrEqual(t, lipgloss.Width(line), maxLineW, "footer line %d", i+1)
	}
	require.True(t, strings.HasSuffix(stripANSI(lines[0]), "262k"), "line0=%q", stripANSI(lines[0]))
	require.True(t, strings.HasSuffix(strings.TrimRight(stripANSI(lines[1]), " "), "[-]"), "line1=%q", stripANSI(lines[1]))
	require.NotContains(t, lines[1], "buildturn")
}
