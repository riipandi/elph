package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

func TestResizeUpdatesViewportDimensions(t *testing.T) {
	m := New()
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 80, Height: 30})
	m = updated.(Model)

	if !m.ready {
		t.Fatal("expected ready after WindowSizeMsg")
	}
	if m.content.Width != 80 {
		t.Fatalf("viewport width %d, want 80", m.content.Width)
	}
	if m.content.Height <= 0 {
		t.Fatal("viewport height must be positive")
	}
	if m.content.Height+m.chromeH > m.height {
		t.Fatalf("viewport %d + chrome %d exceeds terminal %d",
			m.content.Height, m.chromeH, m.height)
	}
}

func TestResizePreservesMessageHistory(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 30
	m.ready = true
	m.messages = []message{{text: "hello from user", kind: msgUser}}

	updated, _ := m.Update(tea.WindowSizeMsg{Width: 60, Height: 24})
	m = updated.(Model)

	if !strings.Contains(m.contentView(), "hello from user") {
		t.Fatal("resize should preserve message history in viewport content")
	}
}

func TestResizeBannerWidthAdapts(t *testing.T) {
	m := New()
	m.width = 120
	wide := lipgloss.Width(m.bannerView())

	m.width = 40
	narrow := lipgloss.Width(m.bannerView())

	if narrow > 40 {
		t.Fatalf("narrow banner %d exceeds terminal width 40", narrow)
	}
	if wide > 120 {
		t.Fatalf("wide banner %d exceeds terminal width 120", wide)
	}
}

func TestResizeBannerWrapsTallerAtNarrowWidth(t *testing.T) {
	m := New()
	m.width = 120
	wide := lipgloss.Height(m.bannerView())

	m.width = 40
	narrow := lipgloss.Height(m.bannerView())

	if narrow <= wide {
		t.Fatalf("expected narrower terminal to wrap banner taller: wide=%d narrow=%d", wide, narrow)
	}
}

func TestManyMessagesContentFitsInViewport(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	for i := range 25 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message number %d from user", i),
			kind: msgUser,
		})
	}

	m = m.syncLayout(true)

	if !strings.Contains(m.contentView(), "message number 24") {
		t.Fatal("expected most recent message in content")
	}
	if m.content.Height < 1 {
		t.Fatal("viewport should have positive height")
	}
}

func TestLongPasteBannerAppearsOnce(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	readme := strings.Repeat("Elph - minimalist AI agent companion. ", 80)
	m.messages = []message{{text: readme, kind: msgUser}}

	content := m.contentView()
	if strings.Count(content, "Welcome to") != 1 {
		t.Fatal("banner should appear exactly once in scrollable content")
	}
}