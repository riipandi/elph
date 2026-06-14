package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/clipboardmedia"
)

type clipboardInitMsg struct{}

func initClipboardCmd() tea.Cmd {
	return func() tea.Msg {
		_ = clipboardmedia.Init()
		return clipboardInitMsg{}
	}
}
