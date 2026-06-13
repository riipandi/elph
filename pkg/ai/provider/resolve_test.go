package provider

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestResolveValueEnvInterpolation(t *testing.T) {
	t.Setenv("MY_API_KEY", "secret-key")

	got, err := ResolveValue("$MY_API_KEY")
	require.NoError(t, err)
	require.Equal(t, "secret-key", got)

	got, err = ResolveValue("${MY_API_KEY}")
	require.NoError(t, err)
	require.Equal(t, "secret-key", got)

	got, err = ResolveValue("env.MY_API_KEY")
	require.NoError(t, err)
	require.Equal(t, "secret-key", got)

	got, err = ResolveValue("prefix-${MY_API_KEY}-suffix")
	require.NoError(t, err)
	require.Equal(t, "prefix-secret-key-suffix", got)

	got, err = ResolveValue("prefix-env.MY_API_KEY-suffix")
	require.NoError(t, err)
	require.Equal(t, "prefix-secret-key-suffix", got)
}

func TestResolveValueEscapes(t *testing.T) {
	got, err := ResolveValue("$$literal")
	require.NoError(t, err)
	require.Equal(t, "$literal", got)

	got, err = ResolveValue("$!literal")
	require.NoError(t, err)
	require.Equal(t, "!literal", got)
}

func TestResolveValueCommand(t *testing.T) {
	got, err := ResolveValue("!echo token-123")
	require.NoError(t, err)
	require.Equal(t, "token-123", got)
}

func TestResolveValueAllowMissingEnv(t *testing.T) {
	got, err := ResolveValueAllowMissingEnv("env.MISSING_API_KEY")
	require.NoError(t, err)
	require.Empty(t, got)

	got, err = ResolveValueAllowMissingEnv("$MISSING_API_KEY")
	require.NoError(t, err)
	require.Empty(t, got)

	t.Setenv("PRESENT", "yes")
	got, err = ResolveValueAllowMissingEnv("env.PRESENT")
	require.NoError(t, err)
	require.Equal(t, "yes", got)
}

func TestIsConfigured(t *testing.T) {
	require.False(t, IsConfigured(""))
	require.True(t, IsConfigured("!echo hi"))
	require.True(t, IsConfigured("$MISSING_VAR"))
	t.Setenv("PRESENT", "yes")
	require.True(t, IsConfigured("$PRESENT"))
}
