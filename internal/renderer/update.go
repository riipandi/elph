package renderer

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// ─── Update ──────────────────────────────────────────────────────────────────

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true

		// Banner(~12) + input(3) + footer(2) + gaps(2) = ~19 initial estimate
		// Actual heights are recalculated in View() on every frame.
		reserved := 12 + 3 + 2 + 2
		vpHeight := msg.Height - reserved
		if vpHeight < 3 {
			vpHeight = 3
		}

		m.vp = viewport.New(msg.Width, vpHeight)
		m.vp.YPosition = 0
		m.vp.Style = lipgloss.NewStyle().Padding(0, 1)

	case ctrlCResetMsg:
		m = m.cancelCtrlC()

	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c":
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && hasInput {
				// Second press, input non-empty → clear input
				m.ctrlCPress = 2
				m.input.SetValue("")
				m.promptChar = ">"

				m = m.replaceNotice("Input cleared, press ctrl+c again to exit")
				return m, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
					return ctrlCResetMsg{}
				})
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				// Third press, or second when input was empty → quit
				m.quitting = true
				return m, tea.Quit
			}

			// First Ctrl+C
			m.ctrlCPress = 1
			m = m.withMessage("Press ctrl+c again to exit")
			m.ctrlCNoticeID = len(m.messages) - 1
			return m, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
				return ctrlCResetMsg{}
			})

		case "ctrl+d":
			m.quitting = true
			return m, tea.Quit

		case "tab":
			m.mode = nextMode(m.mode)
			m = m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))

		case "shift+tab":
			m.mode = prevMode(m.mode)
			m = m.withMessage(fmt.Sprintf("Switched to %s mode", m.mode))

		case "enter":
			// Only submit if textarea is single-line or Ctrl is not held.
			// Ctrl+J is handled by textarea's InsertNewline keymap.
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
			// Strip trigger prefix from submitted value.
			val = stripTrigger(val)
			m = m.addUserMessage(val)
			m.input.SetValue("")
			m.promptChar = ">"
		}

		// Any other key cancels the pending Ctrl+C state.
		m = m.cancelCtrlC()
	}

	// Update input component
	var cmd tea.Cmd
	m.input, cmd = m.input.Update(msg)
	cmds = append(cmds, cmd)

	// Update prompt prefix based on input content.
	m = m.syncPromptPrefix()

	// Update viewport component
	m.vp, cmd = m.vp.Update(msg)
	cmds = append(cmds, cmd)

	return m, tea.Batch(cmds...)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

// addUserMessage appends a user input message (no separators, just | prefix).
func (m Model) addUserMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgUser})
	m.vp.GotoBottom()
	return m
}

// addAIMessage appends an AI response message (| prefix in different color).
func (m Model) addAIMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgAI})
	m.vp.GotoBottom()
	return m
}

// withMessage adds a system/status message (for notices, mode switches, etc).
func (m Model) withMessage(msg string) Model {
	m.messages = append(m.messages, message{text: msg, kind: msgSystem})
	return m
}

// replaceNotice replaces the existing Ctrl+C notice with a new message.
func (m Model) replaceNotice(msg string) Model {
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages[m.ctrlCNoticeID] = message{text: msg, kind: msgSystem}
	} else {
		m.messages = append(m.messages, message{text: msg, kind: msgSystem})
		m.ctrlCNoticeID = len(m.messages) - 1
	}
	return m
}

// cancelCtrlC removes the Ctrl+C notice and resets the press state.
func (m Model) cancelCtrlC() Model {
	m.ctrlCPress = 0
	if m.ctrlCNoticeID >= 0 && m.ctrlCNoticeID < len(m.messages) {
		m.messages = append(m.messages[:m.ctrlCNoticeID], m.messages[m.ctrlCNoticeID+1:]...)
	}
	m.ctrlCNoticeID = -1
	return m
}

// syncPromptPrefix sets the textarea prompt character and color based on input content.
//
//	> normal input
//	/ slash command (starts with /)
//	$ bash/shell command (starts with !)
//	# bash repeat (starts with !!)
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

// stripTrigger removes the command prefix (/, !, !!) from the input.
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
