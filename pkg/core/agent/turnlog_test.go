package agent

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/stretchr/testify/require"
)

func TestLogProviderRequestThinkingState(t *testing.T) {
	var lines []string
	logProviderRequest(func(kind, text string) {
		lines = append(lines, kind+":"+text)
	}, 1, "mimo-v2.5", 3, 5, provider.ThinkingConfig{
		Enabled:        true,
		ThinkingFormat: provider.ThinkingFormatQwen,
	})

	require.Len(t, lines, 1)
	require.Contains(t, lines[0], "provider_start:")
	require.Contains(t, lines[0], "model=mimo-v2.5")
	require.Contains(t, lines[0], "thinking=qwen")
}

func TestWrapThinkingStreamForwardsWithoutBlockingLog(t *testing.T) {
	var forwarded strings.Builder
	handler := wrapThinkingStream(func(kind, text string) {
		t.Fatalf("unexpected log %s:%s", kind, text)
	}, func(chunk string) {
		forwarded.WriteString(chunk)
	})
	handler("alpha")
	handler("beta")
	require.Equal(t, "alphabeta", forwarded.String())
}
