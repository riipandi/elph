package renderer

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/viewport"
	"github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/riipandi/elph/internal/constants"
	"golang.design/x/hotkey"
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
		// Get the key action from our keymap
		action := resolveKeyAction(msg)

		switch action {
		case constants.ActionQuit:
			hasInput := m.input.Value() != ""

			if m.ctrlCPress == 1 && hasInput {
				// Second press, input non-empty → clear input
				m.ctrlCPress = 2
				m.input.SetValue("")
				m.promptChar = ">"
				m = m.replaceNotice("Input cleared, press again to exit")
				return m, tea.Tick(doubleTapTimeout, func(t time.Time) tea.Msg {
					return ctrlCResetMsg{}
				})
			}

			if m.ctrlCPress == 2 || (m.ctrlCPress == 1 && !hasInput) {
				// Third press, or second when input was empty → quit
				m.quitting = true
				return m, tea.Quit
			}

			// First press
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

// ─── Keymap Resolution ─────────────────────────────────────────────────────

// resolveKeyAction maps a tea.KeyMsg to our defined KeyAction.
// It provides a clean separation between raw key events and semantic actions.
func resolveKeyAction(msg tea.KeyMsg) constants.KeyAction {
	// First, check for Ctrl combinations (these have dedicated KeyType constants)
	if msg.Type == tea.KeyCtrlC || msg.Type == tea.KeyCtrlX {
		return constants.ActionQuit
	}
	if msg.Type == tea.KeyCtrlD {
		return constants.ActionExit
	}
	if msg.Type == tea.KeyShiftTab {
		return constants.ActionCycleThink
	}
	if msg.Type == tea.KeyCtrlJ {
		return constants.ActionNewline
	}
	if msg.Type == tea.KeyEnter {
		return constants.ActionSubmit
	}

	// For other keys, check against our keymap bindings
	bindings := constants.KeyBindingsByAction()
	for action, kb := range bindings {
		if matchKeyBinding(msg, kb) {
			return action
		}
	}

	return ""
}

// matchKeyBinding checks if a tea.KeyMsg matches a KeyBinding definition.
func matchKeyBinding(msg tea.KeyMsg, kb constants.KeyBinding) bool {
	// Convert hotkey binding to tea.KeyType for comparison
	targetType := hotkeyKeyType(kb)

	// Check if key type matches
	if targetType == tea.KeyRunes {
		// For letter keys, compare the rune
		if len(msg.Runes) == 1 {
			letter := string(msg.Runes[0])
			targetLetter := hotkeyKeyToLetter(kb.Key)
			return strings.EqualFold(letter, targetLetter)
		}
		return false
	}

	return msg.Type == targetType
}

// hasModifierInList checks if a modifier is in a list of hotkey.Modifier.
func hasModifierInList(modifiers []hotkey.Modifier, target hotkey.Modifier) bool {
	for _, m := range modifiers {
		if m == target {
			return true
		}
	}
	return false
}

// hotkeyKeyType converts a hotkey.KeyBinding to the corresponding tea.KeyType.
func hotkeyKeyType(kb constants.KeyBinding) tea.KeyType {
	hasCtrl := hasModifierInList(kb.Modifiers, hotkey.ModCtrl)
	hasShift := hasModifierInList(kb.Modifiers, hotkey.ModShift)

	// Special combinations
	if hasCtrl && hasShift {
		switch kb.Key {
		case hotkey.KeyTab:
			return tea.KeyShiftTab // Best approximation for Ctrl+Shift+Tab
		}
	}

	if hasCtrl {
		switch kb.Key {
		case hotkey.KeyA:
			return tea.KeyCtrlA
		case hotkey.KeyB:
			return tea.KeyCtrlB
		case hotkey.KeyC:
			return tea.KeyCtrlC
		case hotkey.KeyD:
			return tea.KeyCtrlD
		case hotkey.KeyE:
			return tea.KeyCtrlE
		case hotkey.KeyF:
			return tea.KeyCtrlF
		case hotkey.KeyG:
			return tea.KeyCtrlG
		case hotkey.KeyH:
			return tea.KeyCtrlH
		case hotkey.KeyI:
			return tea.KeyCtrlI
		case hotkey.KeyJ:
			return tea.KeyCtrlJ
		case hotkey.KeyK:
			return tea.KeyCtrlK
		case hotkey.KeyL:
			return tea.KeyCtrlL
		case hotkey.KeyM:
			return tea.KeyCtrlM
		case hotkey.KeyN:
			return tea.KeyCtrlN
		case hotkey.KeyO:
			return tea.KeyCtrlO
		case hotkey.KeyP:
			return tea.KeyCtrlP
		case hotkey.KeyQ:
			return tea.KeyCtrlQ
		case hotkey.KeyR:
			return tea.KeyCtrlR
		case hotkey.KeyS:
			return tea.KeyCtrlS
		case hotkey.KeyT:
			return tea.KeyCtrlT
		case hotkey.KeyU:
			return tea.KeyCtrlU
		case hotkey.KeyV:
			return tea.KeyCtrlV
		case hotkey.KeyW:
			return tea.KeyCtrlW
		case hotkey.KeyX:
			return tea.KeyCtrlX
		case hotkey.KeyY:
			return tea.KeyCtrlY
		case hotkey.KeyZ:
			return tea.KeyCtrlZ
		}
	}

	if hasShift {
		switch kb.Key {
		case hotkey.KeyTab:
			return tea.KeyShiftTab
		}
	}

	// No modifiers - plain keys
	switch kb.Key {
	case hotkey.KeyReturn:
		return tea.KeyEnter
	case hotkey.KeyTab:
		return tea.KeyTab
	case hotkey.KeyEscape:
		return tea.KeyEsc
	case hotkey.KeySpace:
		return tea.KeySpace
	}

	// For letter keys without modifiers, return KeyRunes
	// The actual rune comparison will be done separately
	return tea.KeyRunes
}

// hotkeyKeyToLetter converts a hotkey.Key to its letter representation.
func hotkeyKeyToLetter(k hotkey.Key) string {
	switch k {
	case hotkey.KeyA:
		return "a"
	case hotkey.KeyB:
		return "b"
	case hotkey.KeyC:
		return "c"
	case hotkey.KeyD:
		return "d"
	case hotkey.KeyE:
		return "e"
	case hotkey.KeyF:
		return "f"
	case hotkey.KeyG:
		return "g"
	case hotkey.KeyH:
		return "h"
	case hotkey.KeyI:
		return "i"
	case hotkey.KeyJ:
		return "j"
	case hotkey.KeyK:
		return "k"
	case hotkey.KeyL:
		return "l"
	case hotkey.KeyM:
		return "m"
	case hotkey.KeyN:
		return "n"
	case hotkey.KeyO:
		return "o"
	case hotkey.KeyP:
		return "p"
	case hotkey.KeyQ:
		return "q"
	case hotkey.KeyR:
		return "r"
	case hotkey.KeyS:
		return "s"
	case hotkey.KeyT:
		return "t"
	case hotkey.KeyU:
		return "u"
	case hotkey.KeyV:
		return "v"
	case hotkey.KeyW:
		return "w"
	case hotkey.KeyX:
		return "x"
	case hotkey.KeyY:
		return "y"
	case hotkey.KeyZ:
		return "z"
	default:
		return ""
	}
}
