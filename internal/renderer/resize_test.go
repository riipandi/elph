package renderer

import (
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestRedrawAboveViewCmdLinesUp(t *testing.T) {
	tests := []struct {
		scrollback, view, want int
	}{
		{10, 3, 12},
		{1, 1, 1},
		{0, 5, 4},
		{7, 0, 6},
	}

	for _, tt := range tests {
		got := tt.scrollback + tt.view - 1
		if got != tt.want {
			t.Fatalf("scrollback=%d view=%d: got linesUp %d, want %d", tt.scrollback, tt.view, got, tt.want)
		}
	}
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