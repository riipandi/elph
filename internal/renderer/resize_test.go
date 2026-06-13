package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestResizeUpdatesViewportDimensions(t *testing.T) {
	m := New()
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 80, Height: 30})
	m = updated.(Model)

	require.True(t, m.ready)
	require.Equal(t, 80, m.content.Width)
	require.Positive(t, m.content.Height)
	require.LessOrEqual(t, m.content.Height+m.chromeH, m.height)
}

func TestResizePreservesMessageHistory(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 30
	m.ready = true
	m.messages = []message{{text: "hello from user", kind: constants.MessageUser}}

	updated, _ := m.Update(tea.WindowSizeMsg{Width: 60, Height: 24})
	m = updated.(Model)

	require.Contains(t, m.contentView(), "hello from user")
}

func TestResizeBannerWidthAdapts(t *testing.T) {
	m := New()
	m.width = 120
	wide := lipgloss.Width(m.bannerView())

	m.width = 40
	narrow := lipgloss.Width(m.bannerView())

	require.LessOrEqual(t, narrow, 40)
	require.LessOrEqual(t, wide, 120)
}

func TestResizeBannerWrapsTallerAtNarrowWidth(t *testing.T) {
	m := New()
	m.width = 120
	wide := lipgloss.Height(m.bannerView())

	m.width = 40
	narrow := lipgloss.Height(m.bannerView())

	require.Greater(t, narrow, wide)
}

func TestManyMessagesContentFitsInViewport(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	for i := range 25 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message number %d from user", i),
			kind: constants.MessageUser,
		})
	}

	m = m.syncLayout(true)

	require.Contains(t, m.contentView(), "message number 24")
	require.GreaterOrEqual(t, m.content.Height, 1)
}

func TestLongPasteBannerAppearsOnce(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	readme := strings.Repeat("Elph - minimalist AI agent companion. ", 80)
	m.messages = []message{{text: readme, kind: constants.MessageUser}}

	content := m.contentView()
	require.Equal(t, 1, strings.Count(content, "Welcome to"))
}