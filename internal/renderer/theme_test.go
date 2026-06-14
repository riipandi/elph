package renderer

import (
	"testing"

	"charm.land/lipgloss/v2/compat"
	"github.com/riipandi/elph/internal/theme"
	"github.com/stretchr/testify/require"
)

func TestApplyResolvedThemeForcesLightPalette(t *testing.T) {
	m := testModel()
	m.themePreference = theme.Light

	m = m.applyResolvedTheme(true)
	require.False(t, compat.HasDarkBackground)
	require.Equal(t, "light", glamourStylePath())
}

func TestCycleThemeActionUpdatesPreference(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)

	m := testInputModel(t)
	m.themePreference = theme.Auto

	updated, cmd := m.Update(keyCtrlShiftT())
	m = updated.(Model)
	require.Nil(t, cmd)
	require.Equal(t, theme.Dark, m.themePreference)
	require.True(t, theme.IsDark())
}
