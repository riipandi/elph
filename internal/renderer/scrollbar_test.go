package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestScrollBarHiddenWhenContentFits(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 40
	m.ready = true
	m = m.syncLayout(false)

	require.False(t, m.contentScrollable(), "banner-only content should not be scrollable in tall terminal")
	require.Empty(t, m.contentScrollBarView())
	want := lipgloss.NewStyle().Width(m.width).MaxWidth(m.width).Render(m.content.View())
	require.Equal(t, want, m.contentAreaView(), "content area should equal viewport without gutter")
}

func TestScrollBarVisibleWhenOverflow(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 16
	m.ready = true

	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message line %d with some extra text to wrap nicely", i),
			kind: constants.MessageUser,
		})
	}
	m.contentDirty = true
	m = m.syncLayout(false)

	require.True(t, m.contentScrollable())
	require.Equal(t, m.width-scrollBarWidth, m.contentAreaWidth())
	require.Equal(t, m.contentAreaWidth(), m.content.Width)
	require.LessOrEqual(t, lipgloss.Width(m.contentAreaView()), m.width)

	bar := m.contentScrollBarView()
	require.Equal(t, m.content.Height, lipgloss.Height(bar))
	require.Contains(t, bar, "█")
	require.Contains(t, m.contentAreaView(), "█")
}

func TestScrollBarThumbMovesDown(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 16
	m.ready = true
	for i := range 40 {
		m.messages = append(m.messages, message{text: fmt.Sprintf("msg %d", i), kind: constants.MessageUser})
	}
	m.contentDirty = true
	m = m.syncLayout(false)

	m.content.GotoTop()
	topBar := m.contentScrollBarView()
	topOffset := m.content.YOffset

	m.content.GotoBottom()
	bottomBar := m.contentScrollBarView()
	bottomOffset := m.content.YOffset

	require.Less(t, topOffset, bottomOffset)
	require.NotEqual(t, topBar, bottomBar)
}

func TestContentAreaWidthMatchesChromeWhenScrollable(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 14
	m.ready = true
	for i := range 25 {
		m.messages = append(m.messages, message{text: fmt.Sprintf("overflow %d", i), kind: constants.MessageAI})
	}
	m.contentDirty = true
	m = m.syncLayout(false)

	require.LessOrEqual(t, lipgloss.Width(m.contentAreaView()), m.width)
	require.Equal(t, m.content.Width, m.chromeOuterWidth())
}

func TestInputScrollBarVisibleWhenOverflow(t *testing.T) {
	m := testInputModel(t)
	lines := make([]string, maxInputLines+2)
	for i := range lines {
		lines[i] = fmt.Sprintf("input line %d", i+1)
	}
	m.input.SetValue(strings.Join(lines, "\n"))
	m = m.syncInputWidth()

	require.True(t, m.inputScrollable())
	require.NotEmpty(t, m.inputScrollBarView())
	require.Contains(t, m.inputView(), "█")
}

func TestInputScrollBarHiddenWhenFits(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("short")
	m = m.syncInputWidth()

	require.Empty(t, m.inputScrollBarView())
}