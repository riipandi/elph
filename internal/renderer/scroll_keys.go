package renderer

import (
	"charm.land/bubbles/v2/key"
	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
)

func contentViewportKeyMap() viewport.KeyMap {
	return viewport.KeyMap{
		Up:           key.NewBinding(key.WithKeys("shift+up")),
		Down:         key.NewBinding(key.WithKeys("shift+down")),
		Left:         key.NewBinding(key.WithKeys("shift+left")),
		Right:        key.NewBinding(key.WithKeys("shift+right")),
		PageUp:       key.NewBinding(key.WithKeys("pgup")),
		PageDown:     key.NewBinding(key.WithKeys("pgdown")),
		HalfPageUp:   key.NewBinding(),
		HalfPageDown: key.NewBinding(),
	}
}

func isContentScrollKey(msg tea.KeyPressMsg) bool {
	switch msg.String() {
	case "shift+up", "shift+down", "shift+left", "shift+right", "pgup", "pgdown":
		return true
	}
	return false
}
