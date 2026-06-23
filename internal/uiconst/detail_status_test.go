package uiconst

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestDetailStatusStylesDiffer(t *testing.T) {
	neutral := DetailStatusStyle(DetailStatusNeutral).GetBackground()
	running := DetailStatusStyle(DetailStatusRunning).GetBackground()
	success := DetailStatusStyle(DetailStatusSuccess).GetBackground()
	warning := DetailStatusStyle(DetailStatusWarning).GetBackground()
	errSt := DetailStatusStyle(DetailStatusError).GetBackground()

	require.NotEqual(t, neutral, running)
	require.NotEqual(t, neutral, success)
	require.NotEqual(t, warning, errSt)
}

func TestDetailStatusDiffersFromThinking(t *testing.T) {
	detail := DetailStatusStyle(DetailStatusNeutral).GetBackground()
	thinking := MessageStyle(MessageThinking).GetBackground()
	require.NotEqual(t, detail, thinking)
}

func TestDetailStatusPreviewLabel(t *testing.T) {
	require.Equal(t, "Running...", DetailStatusPreviewLabel(DetailStatusRunning))
	require.Equal(t, "Failed", DetailStatusPreviewLabel(DetailStatusError))
	require.Equal(t, "Cancelled", DetailStatusPreviewLabel(DetailStatusWarning))
	require.Equal(t, "Unavailable", DetailStatusPreviewLabel(DetailStatusUnavailable))
	require.Equal(t, "", DetailStatusPreviewLabel(DetailStatusNeutral))
	require.Equal(t, "", DetailStatusPreviewLabel(DetailStatusSuccess))
}
