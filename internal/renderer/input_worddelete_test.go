package renderer

import (
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/stretchr/testify/require"
)

func TestAltBackspaceDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModAlt})
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestCtrlWDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{Code: 'w', Mod: tea.ModCtrl})
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestMacOptionDeleteTextEncodingDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{
		Code: tea.KeyBackspace,
		Mod:  tea.ModAlt,
		Text: "\x7f",
	})
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestMacCtrlWCodeDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{Code: ctrlWCode, Mod: tea.ModCtrl})
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestEscThenBackspaceDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEscape})
	m = updated.(Model)
	require.True(t, m.inputPendingEsc)

	updated, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyBackspace})
	m = updated.(Model)

	require.False(t, m.inputPendingEsc)
	require.Equal(t, "hello ", m.input.Value())
}

func TestBareCtrlWCodeDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(tea.KeyPressMsg{Code: ctrlWCode})
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestGhosttyAltDeleteDeletesBackwardWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	key := decodeKeyPress(t, []byte("\x1b[3;3~"))
	updated, _ := m.Update(key)
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestGhosttyKittyAltBackspaceDeletesWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	key := decodeKeyPress(t, []byte("\x1b[127;3u"))
	updated, _ := m.Update(key)
	m = updated.(Model)

	require.Equal(t, "hello ", m.input.Value())
}

func TestGhosttyMetaDeleteKillsToEnd(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.SetCursorColumn(5)

	key := decodeKeyPress(t, []byte("\x1b[3;9~"))
	updated, _ := m.Update(key)
	m = updated.(Model)

	require.Equal(t, "hello", m.input.Value())
}

func TestModifyOtherKeysAltBackspaceRawCSI(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(rawCSIMsg([]byte("\x1b[27;3;127~")))
	m = updated.(Model)
	require.Equal(t, "hello ", m.input.Value())
}

func TestFixtermsAltDeleteRawCSIDeletesBackwardWord(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.CursorEnd()

	updated, _ := m.Update(rawCSIMsg([]byte("\x1b[3;3~")))
	m = updated.(Model)
	require.Equal(t, "hello ", m.input.Value())
}

func TestFixtermsMetaDeleteRawCSIKillsToEnd(t *testing.T) {
	m := testInputModel(t)
	m.input.SetValue("hello world")
	m.input.SetCursorColumn(5)

	updated, _ := m.Update(rawCSIMsg([]byte("\x1b[3;9~")))
	m = updated.(Model)
	require.Equal(t, "hello", m.input.Value())
}

func TestWordDeleteMsgFromKey(t *testing.T) {
	msg, ok := wordDeleteMsgFromKey(tea.KeyPressMsg{Code: ctrlWCode, Mod: tea.ModCtrl})
	require.True(t, ok)
	require.Equal(t, deleteWordBackwardKeyMsg(), msg)

	msg, ok = wordDeleteMsgFromKey(decodeKeyPress(t, []byte("\x1b[3;3~")))
	require.True(t, ok)
	require.Equal(t, deleteWordBackwardKeyMsg(), msg)

	msg, ok = wordDeleteMsgFromKey(decodeKeyPress(t, []byte("\x1b[3;9~")))
	require.True(t, ok)
	require.Equal(t, deleteToEndKeyMsg(), msg)
}