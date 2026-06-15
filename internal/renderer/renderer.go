package renderer

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/settings"
	"github.com/riipandi/elph/internal/theme"
	"github.com/riipandi/elph/pkg/ai/provider"
)

// Render starts the TUI application using Bubble Tea.
func Render() error {
	activateTerminalFeaturesSync()
	if err := settings.Ensure(); err != nil {
		return err
	}
	bootstrap, bootstrapped, err := provider.EnsureStarterProviders()
	if err != nil {
		return err
	}
	prefs, err := settings.Load()
	if err != nil {
		prefs = settings.Settings{}
	}
	theme.Apply(theme.Resolve(prefs.ThemeMode(), theme.DetectTerminal()))
	m := New()
	if bootstrapped && len(bootstrap.Created) > 0 {
		m, _ = m.withMessage(fmt.Sprintf(
			"Welcome — created starter providers in %s. Press Ctrl+L to choose a model, then set API keys in those files.",
			providerDirLabel(bootstrap.Dir),
		))
	}
	// Alt screen and mouse mode are declared declaratively in View().
	p := tea.NewProgram(m)
	_, runErr := p.Run()
	return runErr
}

func providerDirLabel(dir string) string {
	dir = strings.TrimSpace(dir)
	if dir == "" {
		return "~/.elph/providers"
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return dir
	}
	if rel, err := filepath.Rel(home, dir); err == nil && !strings.HasPrefix(rel, "..") {
		return filepath.Join("~", rel)
	}
	return dir
}
