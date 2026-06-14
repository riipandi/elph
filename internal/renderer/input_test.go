package renderer

import (
	"fmt"
	"strings"
	"testing"

	"charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func testInputModel(t *testing.T) Model {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	return m.syncLayout(false)
}

func TestCtrlJInsertsNewlineAndGrows(t *testing.T) {
	m := testInputModel(t)

	updated, _ := m.Update(keyCtrlJ())
	m = updated.(Model)

	require.Equal(t, "\n", m.input.Value())
	require.GreaterOrEqual(t, m.input.Height(), 2)
}

func TestEnterSubmitsEvenWhenMultiline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("line one\nline two")
	m = m.syncInputHeight()

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.NotNil(t, cmd)
	require.Len(t, m.messages, 2)
	require.Equal(t, "line one\nline two", m.messages[0].text)
	require.Equal(t, constants.MessageThinking, m.messages[1].kind)
}

func TestMultilinePreservesContentOnSubmit(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("alpha\nbeta")
	m, cmd, ok := m.trySubmitInput()
	require.True(t, ok)
	require.NotNil(t, cmd)
	require.Len(t, m.messages, 2)
	require.Equal(t, "alpha\nbeta", m.messages[0].text)
	require.Equal(t, constants.MessageThinking, m.messages[1].kind)
}

func TestMultilineInputShrinksAfterClear(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("line one\nline two")
	m = m.syncInputHeight()
	require.GreaterOrEqual(t, m.input.Height(), 2)

	m = m.resetInput()
	require.Equal(t, 1, m.input.Height())
}

func TestInputStaysEditableWhileBusy(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")
	updated, _ := m.Update(keyEnter())
	m = updated.(Model)
	require.True(t, m.agent.Busy)
	require.True(t, m.input.Focused())

	updated, cmd := m.Update(keyRune('x'))
	m = updated.(Model)
	require.Nil(t, cmd)
	require.Equal(t, "x", m.input.Value())
}

func TestEnterDoesNotSubmitWhileBusy(t *testing.T) {
	m := testInputModel(t)
	m = m.beginAgentTurn()
	m.input.SetValue("queued message")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.Nil(t, cmd)
	require.True(t, m.agent.Busy)
	require.Equal(t, "queued message", m.input.Value())
	require.Empty(t, m.messages)
}

func TestEnterSubmitsSingleLine(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(keyEnter())
	m = updated.(Model)

	require.NotNil(t, cmd)
	require.True(t, m.agent.Busy)
	require.Len(t, m.messages, 2)
	require.Equal(t, "hello", m.messages[0].text)
	require.Equal(t, constants.MessageThinking, m.messages[1].kind)
	require.Empty(t, m.input.Value())
}

func TestShiftEnterCSIInsertsNewline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(csiMsg("27;2;13~"))
	m = updated.(Model)
	require.Nil(t, cmd)
	require.GreaterOrEqual(t, m.input.LineCount(), 2, "value=%q", m.input.Value())
}

func TestKittyShiftEnterCSIInsertsNewline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, _ := m.Update(csiMsg("13;1u"))
	m = updated.(Model)
	require.GreaterOrEqual(t, m.input.LineCount(), 2, "value=%q", m.input.Value())
}

func TestNormalizeInputTrimsOuterWhitespaceOnly(t *testing.T) {
	require.Equal(t, "hello\n  world", normalizeInputForSubmit("  hello \n  world  \n"))
}

func TestShiftEnterCSIDetection(t *testing.T) {
	cases := []struct {
		payload string
		want    bool
	}{
		{"27;2;13~", true},
		{"27;3;13~", false},
		{"13;1u", true},
		{"13;2u", true},
		{"13;2~", true},
		{"13;5u", true},
	}
	for _, tc := range cases {
		require.Equal(t, tc.want, isShiftEnterMsg(csiMsg(tc.payload)), "payload %q", tc.payload)
	}
}

func TestCtrlJCSIDetection(t *testing.T) {
	cases := []struct {
		payload string
		want    bool
	}{
		{"27;5;10~", true},
		{"27;5;106~", true},
		{"10;4u", true},
		{"106;4u", true},
		{"106;5u", true},
		{"27;2;13~", false},
		{"13;2u", false},
	}
	for _, tc := range cases {
		require.Equal(t, tc.want, isCtrlJPayload(tc.payload), "payload %q", tc.payload)
	}
}

func TestCtrlJCSIInsertsNewline(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(rawCSIMsg([]byte("\x1b[10;4u")))
	m = updated.(Model)
	require.Nil(t, cmd)
	require.GreaterOrEqual(t, m.input.LineCount(), 2, "value=%q", m.input.Value())
}

func TestShiftEnterRawCSIBytes(t *testing.T) {
	raw := []byte("\x1b[27;2;13~")
	require.True(t, isShiftEnterMsg(rawCSIMsg(raw)), "raw xterm CSI: %q", raw)
	require.Equal(t, "27;2;13~", csiPayload(rawCSIMsg(raw)))
}

func TestLiteralNewlineInsertsAndKeepsFirstLine(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello")

	updated, cmd := m.Update(tea.KeyPressMsg{Code: '\n', Text: "\n"})
	m = updated.(Model)
	require.Nil(t, cmd)
	require.True(t, strings.HasPrefix(m.input.Value(), "hello"))
	require.GreaterOrEqual(t, m.input.LineCount(), 2)
	require.GreaterOrEqual(t, m.input.Height(), 2)
}

func TestNewlinePreservesFirstLineWithExistingText(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("first line")

	updated, _ := m.Update(keyCtrlJ())
	m = updated.(Model)

	require.True(t, strings.HasPrefix(m.input.Value(), "first line"))
	require.GreaterOrEqual(t, m.input.Height(), 2)
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
	m.input.SetValue(strings.Repeat("a", m.layout.InputWidth*3))
	h := m.desiredInputHeight()
	require.GreaterOrEqual(t, h, 3)
	require.LessOrEqual(t, h, m.maxInputHeight())
}

func TestNewlineWorksWhenViewportFull(t *testing.T) {
	m := testInputModel(t)
	lines := make([]string, maxInputLines)
	for i := range lines {
		lines[i] = fmt.Sprintf("line %d", i+1)
	}
	m.input.SetValue(strings.Join(lines, "\n"))
	m = m.syncInputHeight()

	require.Equal(t, maxInputLines, m.input.Height())
	require.Equal(t, maxInputLines, m.input.LineCount())

	updated, cmd := m.Update(keyCtrlJ())
	m = updated.(Model)
	require.Nil(t, cmd)
	require.Equal(t, maxInputLines+1, m.input.LineCount(), "value=%q", m.input.Value())
	require.Equal(t, maxInputLines, m.input.Height())
}

func TestMaxInputHeightRespectsTerminal(t *testing.T) {
	m := testInputModel(t)
	m.height = 12
	maxH := m.maxInputHeight()
	require.GreaterOrEqual(t, maxH, 1)
	require.LessOrEqual(t, maxH, maxInputLines)
}
