package renderer

import (
	"regexp"
	"strconv"

	tea "charm.land/bubbletea/v2"
	"charm.land/bubbles/v2/textarea"
)

const (
	ctrlWCode      = 0x17 // EM (^W), common macOS Option+Delete encoding
	xtermMetaMod   = 9    // CSI 3;9~ — Cmd+Delete on Ghostty
	kittyModAlt    = 2
	kittyModMeta   = 8
)

var (
	xtermAltBackspaceRe = regexp.MustCompile(`^27;(\d+);(?:127|8)~$`)
	kittyBackspaceModRe = regexp.MustCompile(`^127;(\d+)u$`)
	fixtermsDeleteModRe = regexp.MustCompile(`^3;(\d+)~$`)
)

func configureInputKeyMap(ta *textarea.Model) {
	keys := append([]string(nil), ta.KeyMap.DeleteWordBackward.Keys()...)
	keys = append(keys, "meta+backspace", "alt+delete")
	ta.KeyMap.DeleteWordBackward.SetKeys(keys...)

	fwd := append([]string(nil), ta.KeyMap.DeleteWordForward.Keys()...)
	fwd = append(fwd, "meta+delete")
	ta.KeyMap.DeleteWordForward.SetKeys(fwd...)
}

func deleteWordBackwardKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModAlt, Text: "alt+backspace"}
}

func deleteWordForwardKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyDelete, Mod: tea.ModAlt, Text: "alt+delete"}
}

func deleteToStartKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'u', Mod: tea.ModCtrl, Text: "ctrl+u"}
}

func deleteToEndKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl, Text: "ctrl+k"}
}

// handleInputWordDelete handles macOS Option/Cmd+Delete in Ghostty, VS Code, and Terminal.
func (m Model) handleInputWordDelete(msg tea.Msg) (Model, bool) {
	if !m.input.Focused() {
		m.inputPendingEsc = false
		return m, false
	}

	if payload := csiPayload(msg); payload != "" {
		if msg, ok := wordDeleteMsgFromCSI(payload); ok {
			m.inputPendingEsc = false
			m.input, _ = m.input.Update(msg)
			return m, true
		}
	}

	key, ok := msg.(tea.KeyPressMsg)
	if !ok {
		return m, false
	}

	// macOS Terminal/Ghostty: ESC then backspace (Meta-Backspace) as two events.
	if isInputEscapeKey(key) {
		m.inputPendingEsc = true
		return m, true
	}
	if m.inputPendingEsc && isBackspaceKey(key) {
		m.inputPendingEsc = false
		m.input, _ = m.input.Update(deleteWordBackwardKeyMsg())
		return m, true
	}
	m.inputPendingEsc = false

	if msg, ok := wordDeleteMsgFromKey(key); ok {
		m.input, _ = m.input.Update(msg)
		return m, true
	}

	return m, false
}

func wordDeleteMsgFromKey(msg tea.KeyPressMsg) (tea.KeyPressMsg, bool) {
	switch msg.Keystroke() {
	case "alt+backspace", "meta+backspace":
		if msg.Mod.Contains(tea.ModMeta) {
			return deleteToStartKeyMsg(), true
		}
		return deleteWordBackwardKeyMsg(), true
	case "alt+delete":
		// Ghostty sends CSI 3;3~ (alt+delete) for Option+Delete on Mac keyboards.
		return deleteWordBackwardKeyMsg(), true
	case "meta+delete", "super+delete":
		return deleteToEndKeyMsg(), true
	case "ctrl+w", "ctrl+\x17":
		return deleteWordBackwardKeyMsg(), true
	}

	if msg.Code == ctrlWCode && msg.Mod == 0 {
		return deleteWordBackwardKeyMsg(), true
	}
	return tea.KeyPressMsg{}, false
}

func wordDeleteMsgFromCSI(payload string) (tea.KeyPressMsg, bool) {
	if m := xtermAltBackspaceRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && xtermModHasAlt(mod) {
			return deleteWordBackwardKeyMsg(), true
		}
	}
	if m := kittyBackspaceModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && mod&kittyModAlt != 0 {
			return deleteWordBackwardKeyMsg(), true
		}
	}
	if m := fixtermsDeleteModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err != nil {
			return tea.KeyPressMsg{}, false
		}
		if mod == xtermMetaMod || mod&kittyModMeta != 0 {
			return deleteToEndKeyMsg(), true
		}
		if xtermModHasAlt(mod) {
			return deleteWordBackwardKeyMsg(), true
		}
	}
	return tea.KeyPressMsg{}, false
}

func isInputEscapeKey(msg tea.KeyPressMsg) bool {
	return msg.Code == tea.KeyEscape && msg.Mod == 0
}

func isBackspaceKey(msg tea.KeyPressMsg) bool {
	return msg.Code == tea.KeyBackspace
}

func isInputEditingKey(msg tea.Msg) bool {
	key, ok := msg.(tea.KeyPressMsg)
	if !ok {
		return false
	}
	return !isContentScrollKey(key)
}

func xtermModHasAlt(mod int) bool {
	return mod > 0 && (mod-1)&2 != 0
}