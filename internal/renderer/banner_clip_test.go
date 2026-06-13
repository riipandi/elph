package renderer

import (
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/stretchr/testify/require"
)

func TestViewHeightFitsTerminal(t *testing.T) {
	for _, h := range []int{20, 24, 30, 40} {
		for _, w := range []int{60, 80, 120} {
			m := New()
			m.width = w
			m.height = h
			m.ready = true
			m = m.syncLayout(false)

			require.LessOrEqual(t, lipgloss.Height(m.View()), h,
				"w=%d h=%d view height exceeds terminal (chrome=%d vp=%d)",
				w, h, m.chromeH, m.content.Height)
		}
	}
}

func TestBannerTopVisibleAtStart(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)

	require.Equal(t, 0, m.content.YOffset)

	vp := m.content.View()
	require.Contains(t, vp, "Welcome to")
}

func TestViewOmitsEmptyActivityLayer(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)

	parts := m.viewParts()
	require.Len(t, parts, 3, "expected 3 view parts without activity")
}