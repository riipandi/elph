package renderer

import (
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/inputui"
)

func (m Model) pasteEditorActive() bool {
	return m.pasteEditor.Active
}

func (m Model) pasteEditorHeight() int {
	if !m.pasteEditorActive() {
		return 0
	}
	view := m.pasteEditorView()
	if view == "" {
		return 0
	}
	return lipgloss.Height(view)
}

func (m Model) pasteEditorView() string {
	if !m.pasteEditorActive() {
		return ""
	}
	border := cachedInputBorder(m.mode)
	boxW := borderedChromeWidth(m.chromeOuterWidth())
	header := dimStyle.Render("Pasted content — ctrl+o or Esc to save")
	body := m.pasteEditor.Input.View()
	inner := lipgloss.JoinVertical(lipgloss.Top, header, body)
	return border.Width(boxW).Render(inner)
}

func (m Model) openPasteEditor(id int) Model {
	text, ok := m.inputPastes[id]
	if !ok {
		return m
	}
	m.input.Blur()
	maxH := min(m.maxInputHeight(), maxInputLines)
	m.pasteEditor = pasteEditorState{
		Active:           true,
		PasteID:          id,
		Input:            inputui.NewPasteEditor(text, m.layout.InputWidth, maxH, noBgStyles),
		SavedInputLine:   m.input.Line(),
		SavedInputColumn: m.input.Column(),
	}
	return m
}

func (m Model) closePasteEditor(save bool) Model {
	if !m.pasteEditorActive() {
		return m
	}
	id := m.pasteEditor.PasteID
	savedLine := m.pasteEditor.SavedInputLine
	savedCol := m.pasteEditor.SavedInputColumn
	if save {
		text := m.pasteEditor.Input.Value()
		m.inputPastes[id] = text
		m = m.replacePasteToken(id, text)
	}
	m.pasteEditor = pasteEditorState{}
	m.input.Focus()
	return m.restoreInputCursorLineCol(savedLine, savedCol)
}

func (m Model) tryOpenPasteEditorAtCursor() (Model, bool) {
	if m.pasteEditorActive() {
		return m.closePasteEditor(true), true
	}
	id, ok := m.pasteIDForEdit()
	if !ok {
		return m, false
	}
	m = m.openPasteEditor(id)
	return m, m.pasteEditorActive()
}

func (m Model) preparePasteEditorHeightForNewline() Model {
	if !m.pasteEditorActive() {
		return m
	}
	maxH := min(m.maxInputHeight(), maxInputLines)
	nextH := min(max(m.pasteEditor.Input.LineCount()+1, 1), maxH)
	if m.pasteEditor.Input.Height() < nextH {
		m.pasteEditor.Input.SetHeight(nextH)
	}
	return m
}

func (m Model) handlePasteEditorNewlineMsg(msg tea.Msg) (Model, tea.Cmd) {
	if !m.pasteEditorActive() {
		return m, nil
	}
	m = m.preparePasteEditorHeightForNewline()
	ctrlJ := tea.KeyPressMsg{Code: 'j', Mod: tea.ModCtrl}
	var cmd tea.Cmd
	switch msg := msg.(type) {
	case tea.KeyPressMsg:
		if isLiteralNewlineKeyMsg(msg) {
			m.pasteEditor.Input, cmd = m.pasteEditor.Input.Update(msg)
		} else {
			m.pasteEditor.Input, cmd = m.pasteEditor.Input.Update(ctrlJ)
		}
	default:
		m.pasteEditor.Input, cmd = m.pasteEditor.Input.Update(ctrlJ)
	}
	if chromeH := m.chromeHeight(); chromeH != m.layout.ChromeH {
		m = m.syncLayout(m.content.AtBottom())
	}
	return m, cmd
}

func (m Model) handlePasteEditorKey(key tea.KeyPressMsg) (Model, tea.Cmd, bool) {
	if !m.pasteEditorActive() {
		return m, nil, false
	}
	if isNewlineInputMsg(key) {
		pasteM, cmd := m.handlePasteEditorNewlineMsg(key)
		return pasteM, cmd, true
	}
	switch key.String() {
	case "esc":
		return m.closePasteEditor(true), nil, true
	}
	if isToggleDetailKey(key) {
		return m.closePasteEditor(true), nil, true
	}
	var cmd tea.Cmd
	m.pasteEditor.Input, cmd = m.pasteEditor.Input.Update(key)
	return m, cmd, true
}

func (m Model) handlePasteToggleKey() (Model, bool) {
	if m.pasteEditorActive() {
		m = m.closePasteEditor(true)
		m = m.syncInputHeight()
		m = m.syncLayout(m.content.AtBottom())
		return m, true
	}
	if m.input.Focused() {
		if updated, ok := m.tryOpenPasteEditorAtCursor(); ok {
			m = updated
			m = m.syncLayout(m.content.AtBottom())
			return m, true
		}
	}
	return m, false
}

//nolint:unused // kept for future use
func (m Model) pasteEditorInputRows() int {
	if !m.pasteEditorActive() {
		return 0
	}
	return inputui.PasteEditorRows(m.pasteEditor.Input.Value(), m.layout.InputWidth)
}
