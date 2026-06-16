package inputui

import (
	"regexp"
	"strconv"

	"charm.land/bubbles/v2/textarea"
	tea "charm.land/bubbletea/v2"
)

const (
	CtrlWCode    = 0x17
	XtermMetaMod = 9
	KittyModAlt  = 2
	KittyModMeta = 8
)

var (
	XtermAltBackspaceRe = regexp.MustCompile(`^27;(\d+);(?:127|8)~$`)
	KittyBackspaceModRe = regexp.MustCompile(`^127;(\d+)u$`)
	FixtermsDeleteModRe = regexp.MustCompile(`^3;(\d+)~$`)
)

// ConfigureKeyMap extends textarea key bindings for platform-specific delete keys.
func ConfigureKeyMap(ta *textarea.Model) {
	keys := append([]string(nil), ta.KeyMap.DeleteWordBackward.Keys()...)
	keys = append(keys, "meta+backspace", "alt+delete")
	ta.KeyMap.DeleteWordBackward.SetKeys(keys...)

	fwd := append([]string(nil), ta.KeyMap.DeleteWordForward.Keys()...)
	fwd = append(fwd, "meta+delete")
	ta.KeyMap.DeleteWordForward.SetKeys(fwd...)
}

// DeleteWordBackwardKeyMsg returns the key message for Option+Delete word delete.
func DeleteWordBackwardKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyBackspace, Mod: tea.ModAlt, Text: "alt+backspace"}
}

//nolint:unused // kept for future use
func deleteWordForwardKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: tea.KeyDelete, Mod: tea.ModAlt, Text: "alt+delete"}
}

func deleteToStartKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'u', Mod: tea.ModCtrl, Text: "ctrl+u"}
}

// DeleteToEndKeyMsg returns the key message for Meta+Delete kill-to-end.
func DeleteToEndKeyMsg() tea.KeyPressMsg {
	return deleteToEndKeyMsg()
}

func deleteToEndKeyMsg() tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl, Text: "ctrl+k"}
}

// WordDeleteMsgFromCSI maps CSI payloads to word-delete key messages.
func WordDeleteMsgFromCSI(payload string) (tea.KeyPressMsg, bool) {
	if m := XtermAltBackspaceRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && xtermModHasAlt(mod) {
			return DeleteWordBackwardKeyMsg(), true
		}
	}
	if m := KittyBackspaceModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && mod&KittyModAlt != 0 {
			return DeleteWordBackwardKeyMsg(), true
		}
	}
	if m := FixtermsDeleteModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err != nil {
			return tea.KeyPressMsg{}, false
		}
		if mod == XtermMetaMod || mod&KittyModMeta != 0 {
			return deleteToEndKeyMsg(), true
		}
		if xtermModHasAlt(mod) {
			return DeleteWordBackwardKeyMsg(), true
		}
	}
	return tea.KeyPressMsg{}, false
}

// WordDeleteMsgFromKey maps key messages to word-delete key messages.
func WordDeleteMsgFromKey(msg tea.KeyPressMsg) (tea.KeyPressMsg, bool) {
	switch msg.Keystroke() {
	case "alt+backspace", "meta+backspace":
		if msg.Mod.Contains(tea.ModMeta) {
			return deleteToStartKeyMsg(), true
		}
		return DeleteWordBackwardKeyMsg(), true
	case "alt+delete":
		return DeleteWordBackwardKeyMsg(), true
	case "meta+delete", "super+delete":
		return deleteToEndKeyMsg(), true
	case "ctrl+w", "ctrl+\x17":
		return DeleteWordBackwardKeyMsg(), true
	}

	if msg.Code == CtrlWCode && msg.Mod == 0 {
		return DeleteWordBackwardKeyMsg(), true
	}
	return tea.KeyPressMsg{}, false
}

// IsInputEscapeKey reports an unmodified escape key press.
func IsInputEscapeKey(msg tea.KeyPressMsg) bool {
	return msg.Code == tea.KeyEscape && msg.Mod == 0
}

// IsBackspaceKey reports backspace key presses.
func IsBackspaceKey(msg tea.KeyPressMsg) bool {
	return msg.Code == tea.KeyBackspace
}

func xtermModHasAlt(mod int) bool {
	return mod > 0 && (mod-1)&2 != 0
}

// IsPasteKey reports clipboard paste key bindings.
func IsPasteKey(msg tea.KeyPressMsg) bool {
	switch msg.String() {
	case "ctrl+v", "meta+v", "cmd+v", "super+v":
		return true
	}
	if msg.Code != 'v' && msg.Code != 'V' {
		return false
	}
	return msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModMeta)
}

func isDeleteOrBackspace(msg tea.KeyPressMsg) bool {
	switch msg.String() {
	case "backspace", "delete", "ctrl+h", "ctrl+backspace", "ctrl+delete",
		"meta+backspace", "meta+delete", "cmd+backspace", "cmd+delete",
		"super+backspace", "super+delete", "shift+backspace", "shift+delete":
		return true
	}
	return msg.Code == tea.KeyBackspace || msg.Code == tea.KeyDelete
}

// IsRemoveLastAttachmentKey is Backspace/Delete with no modifiers.
func IsRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
	if msg.Mod.Contains(tea.ModShift) || msg.Mod.Contains(tea.ModAlt) ||
		msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModMeta) {
		return false
	}
	return isDeleteOrBackspace(msg)
}

// IsCtrlRemoveLastAttachmentKey is Ctrl+Backspace/Delete.
func IsCtrlRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
	if !msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModShift) ||
		msg.Mod.Contains(tea.ModAlt) || msg.Mod.Contains(tea.ModMeta) {
		return false
	}
	switch msg.String() {
	case "ctrl+backspace", "ctrl+delete", "ctrl+h":
		return true
	}
	return isDeleteOrBackspace(msg)
}

// IsClearAttachmentsKey is Shift+Backspace/Delete.
func IsClearAttachmentsKey(msg tea.KeyPressMsg) bool {
	if !msg.Mod.Contains(tea.ModShift) || msg.Mod.Contains(tea.ModAlt) ||
		msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModMeta) {
		return false
	}
	switch msg.String() {
	case "shift+backspace", "shift+delete":
		return true
	}
	return msg.Code == tea.KeyBackspace || msg.Code == tea.KeyDelete
}

// IsMetaClearAttachmentsKey is Cmd/Meta+Backspace/Delete.
func IsMetaClearAttachmentsKey(msg tea.KeyPressMsg) bool {
	switch msg.Keystroke() {
	case "meta+delete", "meta+backspace", "cmd+delete", "cmd+backspace",
		"super+delete", "super+backspace":
		return true
	}
	switch msg.String() {
	case "meta+backspace", "meta+delete", "cmd+backspace", "cmd+delete",
		"super+backspace", "super+delete":
		return true
	}
	if msg.Mod.Contains(tea.ModMeta) && !msg.Mod.Contains(tea.ModAlt) &&
		!msg.Mod.Contains(tea.ModShift) && isDeleteOrBackspace(msg) {
		return true
	}
	return false
}

// IsMetaClearCSIPayload matches raw CSI sequences for Cmd/Meta+Delete.
func IsMetaClearCSIPayload(payload string) bool {
	if m := FixtermsDeleteModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && (mod == XtermMetaMod || mod&KittyModMeta != 0) {
			return true
		}
	}
	if m := XtermAltBackspaceRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && (mod == XtermMetaMod || mod&KittyModMeta != 0) {
			return true
		}
	}
	return false
}
