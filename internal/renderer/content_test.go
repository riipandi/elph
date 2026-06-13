package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestContentViewIncludesBannerAndMessages(t *testing.T) {
	m := New()
	m.width = 80
	m.messages = []message{{text: "hello from user", kind: msgUser}}

	content := m.contentView()
	if !strings.Contains(content, "Welcome to") || !strings.Contains(content, "hello from user") {
		t.Fatalf("content missing banner or message: %q", content)
	}
}

func TestSyncLayoutFitsTerminalHeight(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 30
	m.ready = true

	m = m.syncLayout(false)

	if m.content.Width != 80 {
		t.Fatalf("viewport width %d, want 80", m.content.Width)
	}
	if m.content.Height <= 0 {
		t.Fatal("viewport height must be positive")
	}
	if m.content.Height+m.chromeH > m.height {
		t.Fatalf("viewport %d + chrome %d exceeds terminal height %d",
			m.content.Height, m.chromeH, m.height)
	}
}

func TestContentViewLongPasteIncludesBannerOnce(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	readme := strings.Repeat("Elph minimalist AI agent companion. ", 80)
	m.messages = []message{{text: readme, kind: msgUser}}

	content := m.contentView()
	if strings.Count(content, "Welcome to") != 1 {
		t.Fatal("banner should appear exactly once in scrollable content")
	}
}

func TestBannerWidthMatchesTerminal(t *testing.T) {
	m := New()
	m.width = 50
	m.workDir = strings.Repeat("x", 80)

	banner := m.bannerView()
	if lipgloss.Width(banner) > m.width {
		t.Fatalf("banner wider than terminal: %d > %d", lipgloss.Width(banner), m.width)
	}
}

func TestManyMessagesViewportContent(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true

	for i := range 25 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: msgUser,
		})
	}

	m = m.syncLayout(true)
	if !strings.Contains(m.contentView(), "message 24") {
		t.Fatal("content should include latest messages")
	}
	if m.content.Height < 1 {
		t.Fatal("viewport should have height")
	}
}