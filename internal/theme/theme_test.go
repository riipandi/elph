package theme

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestParseDefaultsUnknownToAuto(t *testing.T) {
	require.Equal(t, Auto, Parse(""))
	require.Equal(t, Auto, Parse("invalid"))
	require.Equal(t, Dark, Parse("dark"))
	require.Equal(t, Light, Parse("light"))
}

func TestResolvePreference(t *testing.T) {
	require.True(t, Resolve(Dark, false))
	require.False(t, Resolve(Light, true))
	require.True(t, Resolve(Auto, true))
	require.False(t, Resolve(Auto, false))
}

func TestNextCyclesModes(t *testing.T) {
	require.Equal(t, Dark, Next(Auto))
	require.Equal(t, Light, Next(Dark))
	require.Equal(t, Auto, Next(Light))
}

func TestApplySetsGlobalMode(t *testing.T) {
	Apply(true)
	require.True(t, IsDark())
	Apply(false)
	require.False(t, IsDark())
}
