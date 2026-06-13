package constants

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestMessageStyleKindsDiffer(t *testing.T) {
	user := MessageStyle(MessageUser).GetForeground()
	tool := MessageStyle(MessageTool).GetForeground()
	require.NotEqual(t, user, tool, "user and tool foreground colors should differ")
}

func TestThinkingUsesDimText(t *testing.T) {
	require.Equal(t, DimText, MessageStyle(MessageThinking).GetForeground(),
		"thinking messages should use dim gray foreground")
}