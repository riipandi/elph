package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/uiconst"
)

func isToggleDetailKey(msg tea.KeyPressMsg) bool {
	if resolveKeyAction(msg) == uiconst.ActionToggleDetail {
		return true
	}
	return (msg.Code == 'o' || msg.Code == 0x0f) && msg.Mod.Contains(tea.ModCtrl)
}

func (m Model) handleToggleDetailKey() (Model, bool) {
	if m.pasteEditorActive() {
		if detailM, handled := m.handlePasteToggleKey(); handled {
			return detailM, true
		}
	}
	if m.input.Focused() && len(pasteIDsInValue(m.input.Value())) > 0 {
		if detailM, handled := m.handlePasteToggleKey(); handled {
			return detailM, true
		}
	}
	if detailM, handled := m.handlePasteToggleKey(); handled {
		return detailM, true
	}
	m, toggled := m.toggleLastDetailExpand()
	if !toggled {
		return m, false
	}
	m = m.syncLayout(false)
	return m, true
}
