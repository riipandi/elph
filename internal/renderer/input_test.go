package renderer

import (
	"strings"
	"testing"

	"github.com/charmbracelet/bubbletea"
)

func testInputModel(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	return m.syncLayout(false)
}

func TestCtrlJInsertsNewlineAndGrows(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlJ})
	m = updated.(Model)

	if m.input.Value() != "\n" {
		t.Fatalf("value %q, want single newline", m.input.Value())
	}
	if m.input.Height() < 2 {
		t.Fatalf("height %d, want at least 2", m.input.Height())
	}
}

func TestEnterSubmitsEvenWhenMultiline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("line one\nline two")
	m = m.syncInputHeight()

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(Model)

	if cmd == nil {
		t.Fatal("enter should submit")
	}
	if len(m.messages) != 1 || m.messages[0].text != "line one\nline two" {
		t.Fatalf("messages = %#v", m.messages)
	}
}

func TestMultilinePreservesContentOnSubmit(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("alpha\nbeta")
	m, cmd, ok := m.trySubmitInput()
	if !ok || cmd == nil {
		t.Fatal("expected multiline submit via trySubmitInput")
	}
	if len(m.messages) != 1 || m.messages[0].text != "alpha\nbeta" {
		t.Fatalf("message = %#v", m.messages)
	}
}

func TestMultilineInputShrinksAfterClear(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("line one\nline two")
	m = m.syncInputHeight()
	if m.input.Height() < 2 {
		t.Fatalf("height %d, want at least 2 before reset", m.input.Height())
	}

	m = m.resetInput()
	if m.input.Height() != 1 {
		t.Fatalf("height %d, want 1 after reset", m.input.Height())
	}
}

func TestEnterSubmitsSingleLine(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(Model)

	if cmd == nil {
		t.Fatal("expected submit command")
	}
	if !m.busy {
		t.Fatal("expected busy after submit")
	}
	if len(m.messages) != 1 || m.messages[0].text != "hello" {
		t.Fatalf("messages = %#v", m.messages)
	}
	if m.input.Value() != "" {
		t.Fatalf("input not cleared: %q", m.input.Value())
	}
}

func TestShiftEnterCSIInsertsNewline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(csiMsg("27;2;13~"))
	m = updated.(Model)
	if cmd != nil {
		t.Fatal("shift+enter should not submit")
	}
	if m.input.LineCount() < 2 {
		t.Fatalf("expected newline from shift+enter CSI, value=%q", m.input.Value())
	}
}

func TestKittyShiftEnterCSIInsertsNewline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, _ := m.Update(csiMsg("13;1u"))
	m = updated.(Model)
	if m.input.LineCount() < 2 {
		t.Fatalf("expected newline from kitty shift+enter, value=%q", m.input.Value())
	}
}

func TestNormalizeInputTrimsOuterWhitespaceOnly(t *testing.T) {
	got := normalizeInputForSubmit("  hello \n  world  \n")
	want := "hello\n  world"
	if got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestShiftEnterCSIDetection(t *testing.T) {
	cases := []struct {
		payload string
		want    bool
	}{
		{"27;2;13~", true},  // xterm modifyOtherKeys shift
		{"27;3;13~", false}, // xterm alt — not shift
		{"13;1u", true},     // kitty shift
		{"13;2u", true},     // Ghostty shift+enter keybind CSI
		{"13;2~", true},     // legacy
		{"13;5u", true},     // kitty shift+ctrl still has shift bit
	}
	for _, tc := range cases {
		if got := isShiftEnterMsg(csiMsg(tc.payload)); got != tc.want {
			t.Fatalf("payload %q: got %v want %v", tc.payload, got, tc.want)
		}
	}
}

func TestShiftEnterRawCSIBytes(t *testing.T) {
	raw := []byte("\x1b[27;2;13~")
	if !isShiftEnterMsg(rawCSIMsg(raw)) {
		t.Fatalf("raw xterm CSI not detected: %q", raw)
	}
	if got := csiPayload(rawCSIMsg(raw)); got != "27;2;13~" {
		t.Fatalf("payload %q, want 27;2;13~", got)
	}
}

func TestLiteralNewlineInsertsAndKeepsFirstLine(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'\n'}})
	m = updated.(Model)
	if cmd != nil {
		t.Fatal("literal newline should not submit")
	}
	if !strings.HasPrefix(m.input.Value(), "hello") {
		t.Fatalf("first line lost: %q", m.input.Value())
	}
	if m.input.LineCount() < 2 {
		t.Fatalf("expected two lines, value=%q", m.input.Value())
	}
	if m.input.Height() < 2 {
		t.Fatalf("height %d, want at least 2 to keep first line visible", m.input.Height())
	}
}

func TestNewlinePreservesFirstLineWithExistingText(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("first line")

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyCtrlJ})
	m = updated.(Model)

	if !strings.HasPrefix(m.input.Value(), "first line") {
		t.Fatalf("first line hidden/lost: %q", m.input.Value())
	}
	if m.input.Height() < 2 {
		t.Fatalf("height %d, want at least 2", m.input.Height())
	}
}

func csiMsg(payload string) csiMsgForTest {
	return csiMsgForTest(payload)
}

type csiMsgForTest string

func (c csiMsgForTest) String() string {
	return "?CSI" + string(c) + "?"
}

func rawCSIMsg(seq []byte) rawCSIMsgForTest {
	return rawCSIMsgForTest(seq)
}

type rawCSIMsgForTest []byte

func TestDesiredInputHeightWrapsLongLine(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue(strings.Repeat("a", m.inputWidth*3))
	h := m.desiredInputHeight()
	if h < 3 {
		t.Fatalf("height %d, want at least 3 for wrapped line", h)
	}
	if h > m.maxInputHeight() {
		t.Fatalf("height %d exceeds max %d", h, m.maxInputHeight())
	}
}

func TestMaxInputHeightRespectsTerminal(t *testing.T) {
	m := testInputModel(t)
	m.height = 12
	maxH := m.maxInputHeight()
	if maxH < 1 {
		t.Fatalf("max height %d must be positive", maxH)
	}
	if maxH > maxInputLines {
		t.Fatalf("max height %d exceeds cap %d", maxH, maxInputLines)
	}
}