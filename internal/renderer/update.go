package renderer

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
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

		if !m.bannerPrinted {
			m.bannerPrinted = true
			cmds = append(cmds, tea.Println(m.bannerView()))
		}

		if m.oldWidth > 0 && m.width > 0 && m.width != m.oldWidth && m.oldView != "" {
			N := strings.Count(m.oldView, "\n")
			M := wrappedHeight(m.oldView, m.width)
			diff := M - N
			cmds = append(cmds, clearAndAdjustCursorCmd(diff))
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
				var cmd tea.Cmd
				m, cmd = m.replaceNotice("Input cleared, press again to exit")
				return m, tea.Batch(cmd, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
					return ctrlCResetMsg{}
				}))
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
			var cmd tea.Cmd
			m, cmd = m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))
			cmds = append(cmds, cmd)

		case constants.ActionCycleThink:
			m.thinkingLevel = constants.NextThinkingLevel(m.thinkingLevel)
			var cmd tea.Cmd
			m, cmd = m.withMessage(fmt.Sprintf("Thinking level: %s", m.thinkingLevel))
			cmds = append(cmds, cmd)

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
			var cmd tea.Cmd
			m, cmd = m.addUserMessage(val)
			cmds = append(cmds, cmd)
			m.input.SetValue("")
			m.promptChar = ">"

		case constants.ActionCopy:
			if len(m.messages) > 0 {
				lastMsg := m.messages[len(m.messages)-1]
				clipboard.Write(clipboard.FmtText, []byte(lastMsg.text))
				var cmd tea.Cmd
				m, cmd = m.withMessage("Copied to clipboard")
				cmds = append(cmds, cmd)
			}
		}

		m = m.cancelCtrlC()
	}

	// Update input component
	var cmd tea.Cmd
	m.input, cmd = m.input.Update(msg)
	cmds = append(cmds, cmd)

	// Update prompt prefix based on input content.
	m = m.syncPromptPrefix()

	// Store old view and old width for resize handling in the next frame
	m.oldView = m.View()
	m.oldWidth = m.width

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

func (m Model) addUserMessage(msg string) (Model, tea.Cmd) {
	newMsg := message{text: msg, kind: msgUser}
	m.messages = append(m.messages, newMsg)
	return m, tea.Println(m.renderMessage(newMsg))
}

func (m Model) addAIMessage(msg string) (Model, tea.Cmd) {
	newMsg := message{text: msg, kind: msgAI}
	m.messages = append(m.messages, newMsg)
	return m, tea.Println(m.renderMessage(newMsg))
}

func (m Model) withMessage(msg string) (Model, tea.Cmd) {
	newMsg := message{text: msg, kind: msgSystem}
	m.messages = append(m.messages, newMsg)
	return m, tea.Println(m.renderMessage(newMsg))
}

func (m Model) replaceNotice(msg string) (Model, tea.Cmd) {
	newMsg := message{text: msg, kind: msgSystem}
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages[m.ctrlCNoticeID] = newMsg
	} else {
		m.messages = append(m.messages, newMsg)
		m.ctrlCNoticeID = len(m.messages) - 1
	}
	return m, tea.Println(m.renderMessage(newMsg))
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

func clearAndAdjustCursorCmd(diff int) tea.Cmd {
	return func() tea.Msg {
		if diff > 0 {
			fmt.Printf("\x1b[%dA", diff)
		} else if diff < 0 {
			fmt.Printf("\x1b[%dB", -diff)
		}
		fmt.Print("\x1b[J")
		return nil
	}
}

func wrappedHeight(s string, width int) int {
	if width <= 0 {
		return 0
	}
	lines := strings.Split(s, "\n")
	totalLines := 0
	for _, line := range lines {
		w := lipgloss.Width(line)
		if w == 0 {
			totalLines += 1
		} else {
			totalLines += (w + width - 1) / width
		}
	}
	return totalLines - 1
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
