package renderer

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/stretchr/testify/require"
)

func TestPasteTokenRegexMatches(t *testing.T) {
	token := pasteToken(0)
	require.True(t, pasteTokenRe.MatchString(token))
	sub := pasteTokenRe.FindStringSubmatch(token)
	require.Len(t, sub, 2)
	require.Equal(t, "0", sub[1])
}

func TestShouldCollapsePaste(t *testing.T) {
	short := "one\ntwo\nthree"
	require.False(t, shouldCollapsePaste(short))
	require.True(t, shouldCollapsePaste(short+"\nfour"))
	require.True(t, shouldCollapsePaste(strings.Repeat("x", pasteCollapseMinRunes)))
}

func TestHandlePasteContentCollapsesLongText(t *testing.T) {
	m := New()
	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")

	updated, handled := m.handlePasteContent(long)
	require.True(t, handled)
	require.Contains(t, updated.input.Value(), "[[paste:0]]")
	require.Equal(t, long, updated.inputPastes[0])
}

func TestUpdatePasteMsgCollapsesLongText(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	updated, _ := m.Update(tea.PasteMsg{Content: long})
	model := updated.(Model)

	require.Contains(t, model.input.Value(), "[[paste:0]]")
	require.Contains(t, pasteDisplayValue(model.input.Value(), model.inputPastes), "[Pasted: 4 lines]")
}

func TestUpdatePasteMsgRespectsUseRawPaste(t *testing.T) {
	m := New()
	m.useRawPaste = true
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	updated, _ := m.Update(tea.PasteMsg{Content: long})
	model := updated.(Model)

	require.Equal(t, long, model.input.Value())
	require.Empty(t, model.inputPastes)
}

func TestUseRawPasteSkipsCollapse(t *testing.T) {
	m := New()
	m.useRawPaste = true

	long := strings.Join([]string{"line 1", "line 2", "line 3", "line 4"}, "\n")
	if !m.useRawPaste && shouldCollapsePaste(long) {
		m = m.insertCollapsedPaste(long)
	} else {
		m = m.insertTextAtCursor(long)
	}

	require.Equal(t, long, m.input.Value())
	require.Empty(t, m.inputPastes)
}

func TestInsertCollapsedPasteShowsPlaceholder(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"line 1", "line 2", "line 3", "line 4"}, "\n")
	m = m.insertCollapsedPaste(long)
	m = m.syncInputHeight()

	require.Contains(t, m.input.Value(), "[[paste:0]]")
	require.Contains(t, pasteDisplayValue(m.input.Value(), m.inputPastes), "[Pasted: 4 lines]")
	require.NotContains(t, m.inputBodyView(), "[[paste:")
	require.Equal(t, long, m.inputPastes[0])
	require.Equal(t, len(m.input.Value()), m.inputCursorOffset())
}

func TestExpandInputPastesOnSubmit(t *testing.T) {
	m := New()
	long := strings.Join([]string{"alpha", "beta", "gamma", "delta"}, "\n")
	m = m.insertCollapsedPaste(long)
	require.True(t, pasteTokenRe.MatchString(m.input.Value()))
	m.input.SetValue("before " + m.input.Value() + " after")
	require.True(t, pasteTokenRe.MatchString(m.input.Value()))

	val := expandInputPastes(m.input.Value(), m.inputPastes)
	require.Equal(t, "before "+long+" after", val)
	require.Equal(t, "before "+long+" after", normalizeInputForSubmit(val))
}

func TestPruneInputPastesRemovesDeletedTokens(t *testing.T) {
	m := New()
	m = m.insertCollapsedPaste("a\nb\nc\nd")
	require.Len(t, m.inputPastes, 1)
	m.input.SetValue("")
	m = m.pruneInputPastes()
	require.Empty(t, m.inputPastes)
}

func TestPasteEditorOpenSaveUpdatesToken(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"one", "two", "three", "four"}, "\n")
	m = m.insertCollapsedPaste(long)
	m.input.CursorStart()

	m = m.openPasteEditor(0)
	require.True(t, m.pasteEditorActive())
	require.Equal(t, long, m.pasteEditor.input.Value())

	edited := strings.Join([]string{"one", "two", "edited"}, "\n")
	m.pasteEditor.input.SetValue(edited)
	m = m.closePasteEditor(true)

	require.Equal(t, edited, m.inputPastes[0])
	require.Contains(t, pasteDisplayValue(m.input.Value(), m.inputPastes), "[Pasted: 3 lines]")
}

func TestUpdateCtrlOOpensPasteEditorAfterPasteMsg(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	updated, _ := m.Update(tea.PasteMsg{Content: long})
	m = updated.(Model)

	updated, _ = m.Update(keyCtrl('o'))
	m = updated.(Model)
	require.True(t, m.pasteEditorActive())
	require.Equal(t, long, m.pasteEditor.input.Value())
}

func TestHandlePasteToggleKeyOpensEditor(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	m = m.insertCollapsedPaste(long)
	m.input.CursorStart()

	updated, handled := m.handlePasteToggleKey()
	require.True(t, handled)
	require.True(t, updated.pasteEditorActive())
}

func testPasteEditorModel(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)
	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	m = m.insertCollapsedPaste(long)
	m = m.openPasteEditor(0)
	require.True(t, m.pasteEditorActive())
	return m
}

func TestPasteEditorCtrlJInsertsNewline(t *testing.T) {
	m := testPasteEditorModel(t)
	m.pasteEditor.input.SetValue("hello")

	updated, cmd := m.Update(keyCtrlJ())
	m = updated.(Model)
	require.Nil(t, cmd)
	require.GreaterOrEqual(t, m.pasteEditor.input.LineCount(), 2, "value=%q", m.pasteEditor.input.Value())
}

func TestPasteEditorShiftEnterCSIInsertsNewline(t *testing.T) {
	m := testPasteEditorModel(t)
	m.pasteEditor.input.SetValue("hello")

	updated, cmd := m.Update(csiMsg("27;2;13~"))
	m = updated.(Model)
	require.Nil(t, cmd)
	require.GreaterOrEqual(t, m.pasteEditor.input.LineCount(), 2, "value=%q", m.pasteEditor.input.Value())
}

func TestPasteEditorRestoresCursorOnClose(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	m = m.insertCollapsedPaste(long)
	m.input.SetValue("hello " + m.input.Value())
	m = m.setInputCursorByteOffset(len("hello "))

	savedLine := m.input.Line()
	savedCol := m.input.Column()

	m = m.openPasteEditor(0)
	require.True(t, m.pasteEditorActive())
	m = m.closePasteEditor(true)

	require.Equal(t, savedLine, m.input.Line())
	require.Equal(t, savedCol, m.input.Column())
}

func TestPasteEditorEscSaves(t *testing.T) {
	m := New()
	m.width = 80
	m.height = 24
	m.ready = true
	m = m.syncLayout(true)

	long := strings.Join([]string{"a", "b", "c", "d"}, "\n")
	m = m.insertCollapsedPaste(long)
	m = m.openPasteEditor(0)
	m.pasteEditor.input.SetValue("saved\nedit\nmore\nlines")

	updated, _, handled := m.handlePasteEditorKey(tea.KeyPressMsg{Code: tea.KeyEscape})
	require.True(t, handled)
	require.False(t, updated.pasteEditorActive())
	require.Equal(t, "saved\nedit\nmore\nlines", updated.inputPastes[0])
}
