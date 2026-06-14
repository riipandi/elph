package settings

import (
	"testing"

	"github.com/riipandi/elph/internal/theme"
	"github.com/stretchr/testify/require"
)

func TestThemeDefaultsToAuto(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, theme.Auto, cfg.ThemeMode())
}

func TestSetThemePersists(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	require.NoError(t, SetTheme(theme.Light))
	cfg, err := Load()
	require.NoError(t, err)
	require.Equal(t, theme.Light, cfg.ThemeMode())
}
