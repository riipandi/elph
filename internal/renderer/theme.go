package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/theme"
)

func requestBackgroundColorCmd() tea.Cmd {
	return func() tea.Msg {
		return tea.RequestBackgroundColor()
	}
}

func (m Model) applyResolvedTheme(terminalDark bool) Model {
	theme.Apply(theme.Resolve(m.themePreference, terminalDark))
	return m.invalidateThemeCaches()
}

func (m Model) invalidateThemeCaches() Model {
	for i := range m.messages {
		m.messages[i].renderCache = messageRenderCache{}
	}
	resetMarkdownCache()
	return m.clearStreamPrefixCache()
}
