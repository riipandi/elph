package renderer

import (
	"fmt"
	"strings"
	"sync"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/atotto/clipboard"
	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
)

// Pre-computed key-binding map for O(1) lookup on every keystroke.
var (
	keyActionMap   map[string]constants.KeyAction
	initKeyMapOnce sync.Once
)

func initKeyMap() {
	keyActionMap = make(map[string]constants.KeyAction, len(constants.DefaultKeyBindings))
	for _, kb := range constants.DefaultKeyBindings {
		if _, exists := keyActionMap[kb.Key]; !exists {
			keyActionMap[kb.Key] = kb.Action
		}
	}
}

func resolveKeyAction(msg tea.KeyPressMsg) constants.KeyAction {
	initKeyMapOnce.Do(initKeyMap)
	if action, ok := keyActionMap[msg.String()]; ok {
		return action
	}
	if action, ok := keyActionMap[msg.Keystroke()]; ok {
		return action
	}
	return ""
}

// ─── Update ──────────────────────────────────────────────────────────────────

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	if key, ok := msg.(tea.KeyPressMsg); ok {
		if m.shell.Running && isShellCancelKey(key) {
			return m.cancelShell()
		}
		if m.agent.Busy && isShellCancelKey(key) {
			return m.cancelAgentTurn()
		}
		if m.modelSelectorActive() {
			var cmd tea.Cmd
			var handled bool
			m, cmd, handled = m.handleModelSelectorKey(key)
			if handled {
				if cmd != nil {
					cmds = append(cmds, cmd)
					return m, tea.Batch(cmds...)
				}
				return m, nil
			}
		}
	}

	if m.input.Focused() && isNewlineInputMsg(msg) {
		m, cmd := m.handleInputNewlineMsg(msg)
		return m, cmd
	}

	if m.input.Focused() {
		if updated, handled := m.handleInputWordDelete(msg); handled {
			m, cmd := updated.finalizeInputEdit()
			return m, cmd
		}
	}

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.layout.ContentDirty = true
		m = m.syncLayout(false)

	case mouseReenableMsg:
		var cmd tea.Cmd
		m, cmd = m.resumeMouseAfterSelection()
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case ctrlCResetMsg:
		m = m.cancelCtrlC()
		m.layout.ContentDirty = true
		m = m.syncLayout(false)

	case agentEventMsg:
		var cmd tea.Cmd
		m, cmd = m.handleAgentEvent(msg)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case agentTurnClosedMsg:
		if m.agent.Busy {
			m.agent.Cancel = nil
			m.agent.Busy = false
			m.agent.Activity = agent.ActivityIdle
			m.agent.SpinnerFrame = 0
			m = m.syncLayout(true)
		}
		m.agent.Events = nil

	case spinnerTickMsg:
		if m.showsActivity() || m.modelsSyncingActive() {
			m.agent.SpinnerFrame++
			if m.modelsSyncingActive() {
				m = m.refreshModelsSyncStatus()
			}
			cmds = append(cmds, m.spinnerTickCmd())
		}

	case shellOutputMsg:
		var cmd tea.Cmd
		m, cmd = m.handleShellOutput(msg)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case shellOutputClosedMsg:
		// Output channel closed; shellDoneMsg follows.

	case shellDoneMsg:
		var cmd tea.Cmd
		m, cmd = m.finishShellDone(msg)
		if cmd != nil {
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case mentionIndexMsg:
		m.suggest.MentionIndexLoading = false
		if msg.workDir == m.workDir {
			m.suggest.MentionIndex = msg.entries
			m.suggest.MentionIndexDir = msg.workDir
		}
		var syncCmd tea.Cmd
		m, syncCmd = m.syncInputSuggestions()
		if syncCmd != nil {
			cmds = append(cmds, syncCmd)
		}

	case termFeaturesMsg:
		// Terminal feature setup complete.

	case modelsSyncDueMsg:
		var cmd tea.Cmd
		m, cmd = m.startModelsSync()
		if cmd != nil {
			cmds = append(cmds, cmd)
		}

	case modelsSyncDoneMsg:
		m = m.finishModelsSync(msg)

	case tea.MouseWheelMsg:
		// Wheel always scrolls the viewport. Resume capture first if a text
		// selection just finished.
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

	case tea.MouseMsg:
		mouse := msg.Mouse()

		var mouseCmds []tea.Cmd
		m, mouseCmds = m.handleMouse(msg)
		cmds = append(cmds, mouseCmds...)
		if m.selectingText {
			return m, tea.Batch(cmds...)
		}

		var cmd tea.Cmd
		if m.isInContentArea(mouse.Y) {
			m.content, cmd = m.content.Update(msg)
			cmds = append(cmds, cmd)
		}
		return m, tea.Batch(cmds...)

	case tea.KeyPressMsg:
		if m.selectingText {
			var cmd tea.Cmd
			m, cmd = m.resumeMouseAfterSelection()
			if cmd != nil {
				cmds = append(cmds, cmd)
			}
		}

		if m.input.Focused() && isInputNewlineKey(msg) {
			m, cmd := m.handleInputNewlineMsg(msg)
			return m, cmd
		}

		if m.input.Focused() {
			var consumed bool
			m, consumed = m.handleInputPaletteKey(msg)
			if consumed {
				return m, nil
			}
		}

		if isContentScrollKey(msg) {
			var cmd tea.Cmd
			m.content, cmd = m.content.Update(msg)
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
			if m.modelSelectorActive() {
				var cmd tea.Cmd
				var handled bool
				m, cmd, handled = m.confirmModelSelector()
				if handled {
					return m, cmd
				}
				break
			}
			if m.agent.Busy || m.shell.Running || !m.input.Focused() {
				break
			}
			var cmd tea.Cmd
			var ok bool
			m, cmd, ok = m.trySubmitInput()
			if ok {
				return m, cmd
			}

		case constants.ActionNewline:
			if !m.input.Focused() {
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

		case constants.ActionOpenModelSelector:
			return m.triggerModelSelector()
		}

		m = m.cancelCtrlC()
	}

	var cmd tea.Cmd
	if !m.input.Focused() || !isInputEditingKey(msg) {
		m.content, cmd = m.content.Update(msg)
		cmds = append(cmds, cmd)
	}

	if m.input.Focused() {
		m.input, cmd = m.input.Update(msg)
		cmds = append(cmds, cmd)
	}

	m, finCmd := m.finalizeInputEdit()
	if finCmd != nil {
		cmds = append(cmds, finCmd)
	}

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageUser})
	m.session.AppendLog("user", text)
	m.layout.ContentDirty = true
	return m
}

func (m Model) addAIMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageAI})
	m.session.AppendLog("ai", text)
	m.layout.ContentDirty = true
	return m
}

func (m Model) addToolMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageTool})
	m.layout.ContentDirty = true
	return m
}

func (m Model) addThinkingMessage(text string) Model {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageThinking})
	m.layout.ContentDirty = true
	return m
}

func (m Model) withMessage(text string) (Model, tea.Cmd) {
	m.messages = append(m.messages, message{text: text, kind: constants.MessageSystem})
	m.session.AppendLog("system", text)
	m.layout.ContentDirty = true
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
	m.layout.ContentDirty = true
	m = m.syncLayout(true)
	return m, nil
}

func (m Model) cancelCtrlC() Model {
	m.ctrlCPress = 0
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages = append(m.messages[:m.ctrlCNoticeID], m.messages[m.ctrlCNoticeID+1:]...)
		m.layout.ContentDirty = true
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
