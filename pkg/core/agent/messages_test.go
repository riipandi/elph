package agent

import (
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestPrepareTurnMessagesAppendsNewUserPromptToHistory(t *testing.T) {
	t.Parallel()

	got := prepareTurnMessages(TurnOptions{
		UserPrompt: "second question",
		Messages: []provider.ChatMessage{
			{Role: "user", Content: "first question"},
			{Role: "assistant", Content: "first answer"},
		},
	})

	require.Len(t, got, 3)
	require.Equal(t, "user", got[2].Role)
	require.Equal(t, "second question", got[2].Content)
}

func TestPrepareTurnMessagesDoesNotDuplicateTrailingUser(t *testing.T) {
	t.Parallel()

	history := []provider.ChatMessage{
		{Role: "user", Content: "only question"},
	}
	got := prepareTurnMessages(TurnOptions{
		UserPrompt: "only question",
		Messages:   history,
	})
	require.Len(t, got, 1)
}

func TestPrepareTurnMessagesUsesPromptWhenHistoryEmpty(t *testing.T) {
	t.Parallel()

	got := prepareTurnMessages(TurnOptions{UserPrompt: "hello"})
	require.Len(t, got, 1)
	require.Equal(t, "hello", got[0].Content)
}