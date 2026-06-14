package renderer

import (
	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/internal/theme"
)

// Render starts the TUI application using Bubble Tea.
func Render() error {
	activateTerminalFeaturesSync()
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}
	theme.Apply(theme.Resolve(prefs.ThemeMode(), theme.DetectTerminal()))
	m := New()
	// Alt screen and mouse mode are declared declaratively in View().
	p := tea.NewProgram(m)
	_, runErr := p.Run()
	return runErr
}
