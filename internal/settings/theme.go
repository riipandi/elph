package settings

import "github.com/riipandi/elph/internal/theme"

// ThemeMode returns the persisted theme preference.
func (s Settings) ThemeMode() theme.Mode {
	return theme.Parse(s.Theme)
}

// SetTheme records the theme preference.
func SetTheme(mode theme.Mode) error {
	return Update(func(cfg *Settings) {
		cfg.Theme = string(theme.Parse(string(mode)))
	})
}
