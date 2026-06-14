package renderer

import tea "charm.land/bubbletea/v2"

func isPasteKey(msg tea.KeyPressMsg) bool {
	switch msg.String() {
	case "ctrl+v", "meta+v", "cmd+v", "super+v":
		return true
	}
	if msg.Code != 'v' && msg.Code != 'V' {
		return false
	}
	return msg.Mod.Contains(tea.ModCtrl) || msg.Mod.Contains(tea.ModMeta)
}
