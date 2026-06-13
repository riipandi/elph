package renderer

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatTokenCount(t *testing.T) {
	require.Equal(t, "128k", formatTokenCount(128000))
	require.Equal(t, "200k", formatTokenCount(200000))
	require.Equal(t, "262k", formatTokenCount(262144))
	require.Equal(t, "16k", formatTokenCount(16384))
	require.Equal(t, "—", formatTokenCount(0))
}
