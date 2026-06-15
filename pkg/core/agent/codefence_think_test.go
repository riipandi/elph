package agent

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestThinkFilterPreservesCodeFenceClosing(t *testing.T) {
	var f ThinkTagStreamFilter
	resp, _ := f.Process("```go\nfmt.Println()\n```")
	require.Equal(t, "```go\nfmt.Println()\n```", resp)
	require.Empty(t, f.holdback)
}

func TestThinkFilterPreservesTrailingInlineBacktick(t *testing.T) {
	var f ThinkTagStreamFilter
	resp, _ := f.Process("code `")
	require.Equal(t, "code `", resp)
	require.Empty(t, f.holdback)
}

func TestThinkFilterFlushRecoversCodeFence(t *testing.T) {
	var f ThinkTagStreamFilter
	full, _ := f.Flush("```go\nfmt.Println()\n```")
	require.Equal(t, "```go\nfmt.Println()\n```", full)
}

func TestThinkFilterStillHoldsPartialQwenOpen(t *testing.T) {
	var f ThinkTagStreamFilter
	resp, _ := f.Process("answer` ")
	require.Equal(t, "answer", resp)
	require.Equal(t, "` ", f.holdback)
}
