package agent

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/ai/protocol"
	"github.com/stretchr/testify/require"
)

func TestTruncateUTF8PreservesCodePoint(t *testing.T) {
	s := strings.Repeat("é", 20) // 2 bytes per char
	got := TruncateUTF8(s, 3)
	require.Equal(t, "é", got)
}

func TestToolResultMessageLimitsLargeOutput(t *testing.T) {
	msg := ToolResultMessage(ToolRunResult{Output: strings.Repeat("x", MaxProviderToolBytes+1024)})
	require.LessOrEqual(t, len(msg), MaxProviderToolBytes+len(truncateNotice))
	require.Contains(t, msg, truncateNotice)
}

func TestCompactMessagesDropsOldestTurn(t *testing.T) {
	var msgs []protocol.ChatMessage
	for i := 0; i < 30; i++ {
		msgs = append(
			msgs,
			protocol.ChatMessage{Role: "user", Content: strings.Repeat("u", 64)},
			protocol.ChatMessage{Role: "assistant", Content: strings.Repeat("a", 64)},
		)
	}
	compact := CompactMessages(msgs)
	require.LessOrEqual(t, len(compact), MaxHistoryMessages)
	require.Equal(t, "user", compact[0].Role)
}

func TestCompactMessagesTruncatesToolPayload(t *testing.T) {
	msgs := []protocol.ChatMessage{{
		Role:    "tool",
		Content: strings.Repeat("o", MaxProviderToolBytes+100),
	}}
	compact := CompactMessages(msgs)
	require.LessOrEqual(t, len(compact[0].Content), MaxProviderToolBytes+len(truncateNotice))
	require.Contains(t, compact[0].Content, truncateNotice)
}
