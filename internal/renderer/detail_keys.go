package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
)

func isToggleDetailKey(msg tea.KeyPressMsg) bool {
	if resolveKeyAction(msg) == constants.ActionToggleDetail {
		return true
	}
	return (msg.Code == 'o' || msg.Code == 0x0f) && msg.Mod.Contains(tea.ModCtrl)
}

func (m Model) handleToggleDetailKey() (Model, bool) {
	if m.pasteEditorActive() {
		if m, handled := m.handlePasteToggleKey(); handled {
			return m, true
		}
	}
	if m.input.Focused() && len(pasteIDsInValue(m.input.Value())) > 0 {
		if m, handled := m.handlePasteToggleKey(); handled {
			return m, true
		}
	}
	if m, handled := m.handlePasteToggleKey(); handled {
		return m, true
	}
	m, toggled := m.toggleLastDetailExpand()
	if !toggled {
		return m, false
	}
	m = m.syncLayout(false)
	return m, true
}
