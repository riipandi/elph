package renderer

import (
	"testing"

	"github.com/charmbracelet/bubbletea"
)

func testModelWithLayout(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(false)
	return m
}

func TestContentClickDisablesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
	m = updated.(Model)

	if m.mouseEnabled {
		t.Fatal("left-click in content should disable mouse capture for selection")
	}
	if !m.selectingText {
		t.Fatal("expected selectingText after content click")
	}
}

func TestInputClickKeepsCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	// Y below the content viewport is the input chrome area.
	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: m.content.Height + 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
	m = updated.(Model)

	if !m.mouseEnabled {
		t.Fatal("click in input area should keep mouse capture for wheel scroll")
	}
	if m.selectingText {
		t.Fatal("input click should not enter selection mode")
	}
}

func TestWheelAfterSelectionResumesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: 1,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonWheelDown,
	})
	m = updated.(Model)

	if !m.mouseEnabled {
		t.Fatal("wheel should resume mouse capture")
	}
	if m.selectingText {
		t.Fatal("wheel should clear selection mode")
	}
}

func TestShiftClickDisablesCapture(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = true

	updated, _ := m.Update(tea.MouseMsg{
		X: 1, Y: m.content.Height + 1, Shift: true,
		Action: tea.MouseActionPress,
		Button: tea.MouseButtonLeft,
	})
	m = updated.(Model)

	if m.mouseEnabled {
		t.Fatal("shift+click should disable mouse capture")
	}
	if !m.selectingText {
		t.Fatal("expected selectingText after shift+click")
	}
}

func TestResumeMouseAfterSelection(t *testing.T) {
	m := New()
	m.mouseEnabled = false
	m.selectingText = true

	updated, cmd := m.resumeMouseAfterSelection()
	m = updated

	if !m.mouseEnabled {
		t.Fatal("expected mouse re-enabled")
	}
	if m.selectingText {
		t.Fatal("expected selectingText cleared")
	}
	if cmd == nil {
		t.Fatal("expected EnableMouseCellMotion command")
	}
}

func TestKeyAfterSelectionResumesMouse(t *testing.T) {
	m := testModelWithLayout(t)
	m.mouseEnabled = false
	m.selectingText = true

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'a'}})
	m = updated.(Model)

	if !m.mouseEnabled {
		t.Fatal("key press should resume mouse capture")
	}
	if m.selectingText {
		t.Fatal("selectingText should be cleared after key press")
	}
}