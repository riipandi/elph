package renderer

import (
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"golang.design/x/clipboard"
)

// Pre-computed key-binding map for O(1) lookup on every keystroke.
var (
	keyActionMap   map[tea.KeyType]constants.KeyAction
	initKeyMapOnce sync.Once
)

func initKeyMap() {
	keyActionMap = make(map[tea.KeyType]constants.KeyAction, len(constants.DefaultKeyBindings))
	for _, kb := range constants.DefaultKeyBindings {
		if _, exists := keyActionMap[kb.Type]; !exists {
			keyActionMap[kb.Type] = kb.Action
		}
	}
}

func resolveKeyAction(msg tea.KeyMsg) constants.KeyAction {
	initKeyMapOnce.Do(initKeyMap)
	if action, ok := keyActionMap[msg.Type]; ok {
		return action
	}
	return ""
}

// ─── Update ──────────────────────────────────────────────────────────────────

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.contentDirty = true
		m = m.syncLayout(false)

	case mouseReenableMsg:
		var cmd tea.Cmd
		m, cmd = m.resumeMouseAfterSelection()
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case ctrlCResetMsg:
		m = m.cancelCtrlC()
		m.contentDirty = true
		m = m.syncLayout(false)

	case tea.MouseMsg:
		evt := tea.MouseEvent(msg)

		// Wheel always scrolls the viewport. Resume capture first if a text
		// selection just finished.
		if evt.IsWheel() {
			if m.selectingText || !m.mouseEnabled {
				var cmd tea.Cmd
				m, cmd = m.resumeMouseAfterSelection()
				if cmd != nil {
					cmds = append(cmds, cmd)
				}
			}
			var cmd tea.Cmd
			m.content, cmd = m.content.Update(msg)
			cmds = append(cmds, cmd)
			return m, tea.Batch(cmds...)
		}

		var mouseCmds []tea.Cmd
		m, mouseCmds = m.handleMouse(msg)
		cmds = append(cmds, mouseCmds...)
		if m.selectingText {
			return m, tea.Batch(cmds...)
		}

		var cmd tea.Cmd
		if m.isInContentArea(evt.Y) {
			m.content, cmd = m.content.Update(msg)
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case tea.KeyMsg:
		if m.selectingText {
			var cmd tea.Cmd
			m, cmd = m.resumeMouseAfterSelection()
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		}

		action := resolveKeyAction(msg)

		switch action {
		case constants.ActionQuit:
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && hasInput {
				m.ctrlCPress = 2
				m.input.SetValue("")
				m.promptChar = ">"
				var cmd tea.Cmd
				m, cmd = m.replaceNotice("Input cleared, press again to exit")
				return m, cmd
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				m.quitting = true
				return m, tea.Quit
			}

			m.ctrlCPress = 1
			var cmd tea.Cmd
			m, cmd = m.withMessage("Press again to exit")
			m.ctrlCNoticeID = len(m.messages) - 1
			return m, tea.Batch(cmd, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
				return ctrlCResetMsg{}
			}))

		case constants.ActionExit:
			m.quitting = true
			return m, tea.Quit

		case constants.ActionSwitchMode:
			m.mode = nextMode(m.mode)
			m, cmd := m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))
			return m, cmd

		case constants.ActionCycleThink:
			m.thinkingLevel = constants.NextThinkingLevel(m.thinkingLevel)
			m, cmd := m.withMessage(fmt.Sprintf("Thinking level: %s", m.thinkingLevel))
			return m, cmd

		case constants.ActionSubmit:
			if !m.input.Focused() {
				break
			}
			val := strings.TrimSpace(m.input.Value())
			if val == "" {
				break
			}
			if val == ":q" || val == ":q!" {
				m.quitting = true
				return m, tea.Quit
			}
			val = stripTrigger(val)
			m = m.addUserMessage(val)
			m.input.SetValue("")
			m.promptChar = ">"
			m = m.syncLayout(true)

		case constants.ActionCopy:
			if len(m.messages) > 0 {
				lastMsg := m.messages[len(m.messages)-1]
				clipboard.Write(clipboard.FmtText, []byte(lastMsg.text))
				m, cmd := m.withMessage("Copied to clipboard")
				return m, cmd
			}
		}

		m = m.cancelCtrlC()
	}

	var cmd tea.Cmd
	m.content, cmd = m.content.Update(msg)
	cmds = append(cmds, cmd)

	m.input, cmd = m.input.Update(msg)
	cmds = append(cmds, cmd)

	prevPrefix := m.showPromptPrefix
	m = m.syncPromptPrefix()
	if m.showPromptPrefix != prevPrefix {
		m = m.syncInputWidth()
	}

	// Re-layout when the input area grows or shrinks (multiline).
	chromeH := lipgloss.Height(m.inputView()) + lipgloss.Height(m.footerView())
	if chromeH != m.chromeH {
		m = m.syncLayout(m.content.AtBottom())
	}

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: msgUser})
	m.contentDirty = true
	return m
}

func (m Model) addAIMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: msgAI})
	m.contentDirty = true
	return m
}

func (m Model) withMessage(text string) (Model, tea.Cmd) {
	m.messages = append(m.messages, message{text: text, kind: msgSystem})
	m.contentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) replaceNotice(text string) (Model, tea.Cmd) {
	newMsg := message{text: text, kind: msgSystem}
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages[m.ctrlCNoticeID] = newMsg
	} else {
		m.messages = append(m.messages, newMsg)
		m.ctrlCNoticeID = len(m.messages) - 1
	}
	m.contentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) cancelCtrlC() Model {
	m.ctrlCPress = 0
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages = append(m.messages[:m.ctrlCNoticeID], m.messages[m.ctrlCNoticeID+1:]...)
		m.contentDirty = true
	}
	m.ctrlCNoticeID = -1
	return m
}

func (m Model) syncPromptPrefix() Model {
	trimmed := strings.TrimLeft(m.input.Value(), " ")

	if trimmed == "" {
		m.promptChar = ">"
		return m
	}

	switch {
	case strings.HasPrefix(trimmed, "!!"):
		m.promptChar = "#"
	case strings.HasPrefix(trimmed, "!"):
		m.promptChar = "$"
	case strings.HasPrefix(trimmed, "/"):
		m.promptChar = "/"
	}

	return m
}

func stripTrigger(s string) string {
	s = strings.TrimLeft(s, " ")
	switch {
	case strings.HasPrefix(s, "!!"):
		return strings.TrimPrefix(s, "!!")
	case strings.HasPrefix(s, "!"):
		return strings.TrimPrefix(s, "!")
	case strings.HasPrefix(s, "/"):
		return strings.TrimPrefix(s, "/")
	}
	return s
}