package renderer

import (
	"strconv"

	tea "charm.land/bubbletea/v2"
)

func isDeleteOrBackspace(msg tea.KeyPressMsg) bool {
	switch msg.String() {
	case "backspace", "delete", "ctrl+h", "ctrl+backspace", "ctrl+delete",
		"meta+backspace", "meta+delete", "cmd+backspace", "cmd+delete",
		"super+backspace", "super+delete", "shift+backspace", "shift+delete":
		return true
	}
	return msg.Code == tea.KeyBackspace || msg.Code == tea.KeyDelete
}

// isRemoveLastAttachmentKey is Backspace/Delete with no modifiers.
func isRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
	if msg.Mod.Contains(tea.ModShift) || msg.Mod.Contains(tea.ModAlt) ||
		msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModMeta) {
		return false
	}
	return isDeleteOrBackspace(msg)
}

// isCtrlRemoveLastAttachmentKey is Ctrl+Backspace/Delete — remove last attachment.
func isCtrlRemoveLastAttachmentKey(msg tea.KeyPressMsg) bool {
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

// isClearAttachmentsKey is Shift+Backspace/Delete — clear all attachments.
func isClearAttachmentsKey(msg tea.KeyPressMsg) bool {
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

// isMetaClearAttachmentsKey is Cmd/Meta+Backspace/Delete — clear all attachments.
func isMetaClearAttachmentsKey(msg tea.KeyPressMsg) bool {
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

// isMetaClearCSIPayload matches raw CSI sequences for Cmd/Meta+Delete (e.g. Ghostty 3;9~).
func isMetaClearCSIPayload(payload string) bool {
	if m := fixtermsDeleteModRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && (mod == xtermMetaMod || mod&kittyModMeta != 0) {
			return true
		}
	}
	if m := xtermAltBackspaceRe.FindStringSubmatch(payload); len(m) == 2 {
		mod, err := strconv.Atoi(m[1])
		if err == nil && (mod == xtermMetaMod || mod&kittyModMeta != 0) {
			return true
		}
	}
	return false
}
