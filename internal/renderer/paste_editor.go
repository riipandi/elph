package renderer

import (
	"strings"

	"charm.land/bubbles/v2/textarea"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

type pasteEditorState struct {
	Active           bool
	PasteID          int
	input            textarea.Model
	savedInputLine   int
	savedInputColumn int
}

func (m Model) pasteEditorActive() bool {
	return m.pasteEditor.Active
}

func newPasteEditor(text string, width, maxHeight int) textarea.Model {
	ta := textarea.New()
	ta.SetValue(text)
	ta.Prompt = ""
	ta.Placeholder = ""
	ta.ShowLineNumbers = false
	ta.CharLimit = 0
	ta.SetStyles(noBgStyles())
	ta.KeyMap.InsertNewline.SetKeys("ctrl+j", "shift+enter")
	configureInputKeyMap(&ta)
	ta.SetWidth(max(width, 1))
	lines := pasteLineCount(text)
	h := min(max(lines, 1), max(maxHeight, 1))
	ta.SetHeight(h)
	ta.Focus()
	return ta
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
	body := m.pasteEditor.input.View()
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
		input:            newPasteEditor(text, m.layout.InputWidth, maxH),
		savedInputLine:   m.input.Line(),
		savedInputColumn: m.input.Column(),
	}
	return m
}

func (m Model) closePasteEditor(save bool) Model {
	if !m.pasteEditorActive() {
		return m
	}
	id := m.pasteEditor.PasteID
	savedLine := m.pasteEditor.savedInputLine
	savedCol := m.pasteEditor.savedInputColumn
	if save {
		text := m.pasteEditor.input.Value()
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
	nextH := min(max(m.pasteEditor.input.LineCount()+1, 1), maxH)
	if m.pasteEditor.input.Height() < nextH {
		m.pasteEditor.input.SetHeight(nextH)
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
			m.pasteEditor.input, cmd = m.pasteEditor.input.Update(msg)
		} else {
			m.pasteEditor.input, cmd = m.pasteEditor.input.Update(ctrlJ)
		}
	default:
		m.pasteEditor.input, cmd = m.pasteEditor.input.Update(ctrlJ)
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
		m, cmd := m.handlePasteEditorNewlineMsg(key)
		return m, cmd, true
	}
	switch key.String() {
	case "esc":
		return m.closePasteEditor(true), nil, true
	}
	if isToggleDetailKey(key) {
		return m.closePasteEditor(true), nil, true
	}
	var cmd tea.Cmd
	m.pasteEditor.input, cmd = m.pasteEditor.input.Update(key)
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

func (m Model) pasteEditorInputRows() int {
	if !m.pasteEditorActive() {
		return 0
	}
	val := m.pasteEditor.input.Value()
	if val == "" {
		return 1
	}
	w := max(m.layout.InputWidth, 1)
	rows := 0
	for _, line := range strings.Split(val, "\n") {
		rows += wrappedInputRows(line, w)
	}
	return max(rows, 1)
}
