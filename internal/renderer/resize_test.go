package renderer

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestScrollbackGeometry(t *testing.T) {
	tests := []struct {
		name                      string
		onScreen, newTotal        int
		wantClear, wantCursorUp   int
	}{
		{"grew scrollback", 15, 18, 18, 14},
		{"shrank scrollback", 15, 11, 15, 14},
		{"no prior scrollback", 4, 14, 14, 3},
		{"single line", 2, 2, 2, 1},
	}

	for _, tt := range tests {
		clearTotal, cursorUp := scrollbackGeometry(tt.onScreen, tt.newTotal)
		if clearTotal != tt.wantClear || cursorUp != tt.wantCursorUp {
			t.Fatalf("%s: got clear=%d cursorUp=%d, want clear=%d cursorUp=%d",
				tt.name, clearTotal, cursorUp, tt.wantClear, tt.wantCursorUp)
		}
	}
}

func TestStreamViewIncludesMessageHistory(t *testing.T) {
	m := New()
	m.width = 80
	m.messages = []message{{text: "hello from user", kind: msgUser}}

	stream := m.streamView()
	if !containsAll(stream, "Welcome to", "hello from user") {
		t.Fatalf("streamView missing banner or history: %q", stream)
	}
}

func containsAll(s string, parts ...string) bool {
	for _, p := range parts {
		if !strings.Contains(s, p) {
			return false
		}
	}
	return true
}

func TestStreamViewHeightChangesWithWidth(t *testing.T) {
	m := New()
	m.width = 120

	wide := lipgloss.Height(m.streamView())

	m.width = 40
	narrow := lipgloss.Height(m.streamView())

	if narrow <= wide {
		t.Fatalf("expected narrower terminal to wrap banner taller: wide=%d narrow=%d", wide, narrow)
	}
}

func TestScrollbackLineCount(t *testing.T) {
	if got := scrollbackLineCount("one\ntwo\n"); got != 2 {
		t.Fatalf("got %d lines, want 2", got)
	}
	if got := scrollbackLineCount(""); got != 0 {
		t.Fatalf("empty content should be 0 lines, got %d", got)
	}
}