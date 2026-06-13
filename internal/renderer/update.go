package renderer

import (
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/atotto/clipboard"
	"github.com/charmbracelet/bubbletea"
	"github.com/riipandi/elph/internal/constants"
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

	if !m.busy && m.input.Focused() && isNewlineInputMsg(msg) {
		m, cmd := m.handleInputNewlineMsg(msg)
		return m, cmd
	}

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

	case ActivityMsg:
		m.activity = msg.Activity
		m = m.syncLayout(m.content.AtBottom())

	case spinnerTickMsg:
		if m.busy {
			m.spinnerFrame++
			cmds = append(cmds, m.spinnerTickCmd())
		}

	case AgentDoneMsg:
		m = m.finishAgentTurn(msg.Response)

	case termFeaturesMsg:
		// Terminal feature setup complete.

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

		if !m.busy && m.input.Focused() && isInputNewlineKey(msg) {
			m, cmd := m.handleInputNewlineMsg(msg)
			return m, cmd
		}

		action := resolveKeyAction(msg)

		switch action {
		case constants.ActionQuit:
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && hasInput {
				m.ctrlCPress = 2
				m = m.resetInput()
				var cmd tea.Cmd
				m, cmd = m.replaceNotice("Input cleared, press again to exit")
				return m, cmd
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				m.quitting = true
				return m, tea.Sequence(disableTerminalFeatures(), tea.Quit)
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
			return m, tea.Sequence(disableTerminalFeatures(), tea.Quit)

		case constants.ActionSwitchMode:
			m.mode = nextMode(m.mode)
			m, cmd := m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))
			return m, cmd

		case constants.ActionCycleThink:
			m.thinkingLevel = constants.NextThinkingLevel(m.thinkingLevel)
			m, cmd := m.withMessage(fmt.Sprintf("Thinking level: %s", m.thinkingLevel))
			return m, cmd

		case constants.ActionSubmit:
			if isInputNewlineKey(msg) {
				break
			}
			if m.busy || !m.input.Focused() {
				break
			}
			var cmd tea.Cmd
			var ok bool
			m, cmd, ok = m.trySubmitInput()
			if ok {
				return m, cmd
			}

		case constants.ActionNewline:
			if m.busy || !m.input.Focused() {
				break
			}
			m, cmd := m.handleInputNewlineMsg(msg)
			return m, cmd

		case constants.ActionCopy:
			if len(m.messages) > 0 {
				lastMsg := m.messages[len(m.messages)-1]
				_ = clipboard.WriteAll(lastMsg.text)
				m, cmd := m.withMessage("Copied to clipboard")
				return m, cmd
			}
		}

		m = m.cancelCtrlC()
	}

	var cmd tea.Cmd
	m.content, cmd = m.content.Update(msg)
	cmds = append(cmds, cmd)

	if !m.busy {
		prevInputH := m.input.Height()
		m.input, cmd = m.input.Update(msg)
		cmds = append(cmds, cmd)
		m = m.syncInputHeight()
		if m.input.Height() != prevInputH {
			m = m.syncInputWidth()
		}
	}

	prevPrefix := m.showPromptPrefix
	m = m.syncPromptPrefix()
	if m.showPromptPrefix != prevPrefix {
		m = m.syncInputWidth()
	}

	// Re-layout when chrome height changes (activity, multiline input, etc.).
	chromeH := m.chromeHeight()
	if chromeH != m.chromeH {
		m = m.syncLayout(m.content.AtBottom())
	}

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageUser})
	m.contentDirty = true
	return m
}

func (m Model) addAIMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageAI})
	m.contentDirty = true
	return m
}

func (m Model) addToolMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageTool})
	m.contentDirty = true
	return m
}

func (m Model) addThinkingMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageThinking})
	m.contentDirty = true
	return m
}

func (m Model) withMessage(text string) (Model, tea.Cmd) {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageSystem})
	m.contentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) replaceNotice(text string) (Model, tea.Cmd) {
	newMsg := message{text: text, kind: constants.MessageSystem}
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