package renderer

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestBannerMetadataLineClamp(t *testing.T) {
	m := New()
	m.width = 50
	m.workDir = strings.Repeat("x", 80)

	banner := m.bannerView()
	if lipgloss.Width(banner) > m.width {
		t.Fatalf("banner wider than terminal: banner=%d terminal=%d", lipgloss.Width(banner), m.width)
	}

	if strings.Contains(banner, strings.Repeat("x", 80)) {
		t.Fatal("directory value was not line-clamped")
	}
	if !strings.Contains(banner, "Directory:") {
		t.Fatal("directory metadata line not found")
	}
}

func TestBannerTipWraps(t *testing.T) {
	m := New()
	m.width = 40
	m.tip = strings.Repeat("word ", 30)

	banner := m.bannerView()
	if lipgloss.Height(banner) < 12 {
		t.Fatalf("expected tip to wrap to multiple lines, height=%d", lipgloss.Height(banner))
	}
}

func TestFooterLineClamp(t *testing.T) {
	m := New()
	m.width = 42
	m.modelName = "Claude Sonnet 4.6 Extended Edition"

	footer := m.footerView()
	if lipgloss.Width(footer) > m.width {
		t.Fatalf("footer wider than terminal: footer=%d terminal=%d", lipgloss.Width(footer), m.width)
	}

	lines := strings.Split(strings.TrimSpace(footer), "\n")
	if len(lines) != 2 {
		t.Fatalf("expected 2 footer lines, got %d", len(lines))
	}
	if lipgloss.Width(lines[0]) > footerContentWidth(m.width)+1 {
		t.Fatalf("footer line 1 exceeds content width: %d > %d", lipgloss.Width(lines[0]), footerContentWidth(m.width)+1)
	}
}