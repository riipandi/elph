package renderer

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/bubbletea"
	"github.com/riipandi/elph/internal/constants"
	"golang.design/x/clipboard"
)

// ─── Update ──────────────────────────────────────────────────────────────────

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		
		// Only create viewport once, then resize
		if m.vp.Width == 0 || m.vp.Height == 0 {
			vpHeight := max(m.height-18, 3)
			m.vp = viewport.New(msg.Width, vpHeight)
		} else {
			m.vp.Width = msg.Width
			m.vp.Height = max(m.height-18, 3)
		}
	case ctrlCResetMsg:
		m = m.cancelCtrlC()

	case tea.KeyMsg:
		action := resolveKeyAction(msg)

		switch action {
		case constants.ActionQuit:
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && hasInput {
				m.ctrlCPress = 2
				m.input.SetValue("")
				m.promptChar = ">"
				m = m.replaceNotice("Input cleared, press again to exit")
				return m, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
					return ctrlCResetMsg{}
				})
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				m.quitting = true
				return m, tea.Quit
			}

			m.ctrlCPress = 1
			m = m.withMessage("Press again to exit")
			m.ctrlCNoticeID = len(m.messages) - 1
			return m, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
				return ctrlCResetMsg{}
			})

		case constants.ActionExit:
			m.quitting = true
			return m, tea.Quit

		case constants.ActionSwitchMode:
			m.mode = nextMode(m.mode)
			m = m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))

		case constants.ActionCycleThink:
			m.thinkingLevel = constants.NextThinkingLevel(m.thinkingLevel)
			m = m.withMessage(fmt.Sprintf("Thinking level: %s", m.thinkingLevel))

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

		case constants.ActionCopy:
			if len(m.messages) > 0 {
				lastMsg := m.messages[len(m.messages)-1]
				clipboard.Write(clipboard.FmtText, []byte(lastMsg.text))
				m = m.withMessage("Copied to clipboard")
			}
		}

		m = m.cancelCtrlC()
	}

	// Update viewport FIRST (before textarea consumes keys)
	var cmd tea.Cmd
	m.vp, cmd = m.vp.Update(msg)
	cmds = append(cmds, cmd)

	// Update input component
	m.input, cmd = m.input.Update(msg)
	cmds = append(cmds, cmd)

	// Update prompt prefix based on input content.
	m = m.syncPromptPrefix()

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgUser})
	m.vp.GotoBottom()
	return m
}

func (m Model) addAIMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgAI})
	m.vp.GotoBottom()
	return m
}

func (m Model) withMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgSystem})
	m.vp.GotoBottom()
	return m
}

func (m Model) replaceNotice(msg string) Model {
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages[m.ctrlCNoticeID] = message{text: msg, kind: msgSystem}
	} else {
		m.messages = append(m.messages, message{text: msg, kind: msgSystem})
		m.ctrlCNoticeID = len(m.messages) - 1
	}
	return m
}

func (m Model) cancelCtrlC() Model {
	m.ctrlCPress = 0
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages = append(m.messages[:m.ctrlCNoticeID], m.messages[m.ctrlCNoticeID+1:]...)
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

// ─── Keymap Resolution ─────────────────────────────────────────────────────

// resolveKeyAction maps a tea.KeyMsg to our defined KeyAction.
func resolveKeyAction(msg tea.KeyMsg) constants.KeyAction {
	for _, kb := range constants.DefaultKeyBindings {
		if msg.Type == kb.Type {
			return kb.Action
		}
	}
	return ""
}
