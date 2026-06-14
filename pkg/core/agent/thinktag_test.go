package agent

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestThinkTagStreamFilterThinkTags(t *testing.T) {
	var f ThinkTagStreamFilter

	resp, think := f.Process("a<think>one</think>b<think>two</think>c")
	require.Equal(t, "abc", resp)
	require.Equal(t, "onetwo", think)
}

func TestThinkTagStreamFilterQwenDelimiters(t *testing.T) {
	var f ThinkTagStreamFilter

	resp, think := f.Process("a` <think>step` </think>b")
	require.Equal(t, "ab", resp)
	require.Equal(t, "step", think)
}

func TestThinkTagStreamFilterRedactedTags(t *testing.T) {
	var f ThinkTagStreamFilter

	resp, think := f.Process("a<redacted_thinking>one</redacted_thinking>b")
	require.Equal(t, "ab", resp)
	require.Equal(t, "one", think)
}

func TestThinkTagStreamFilterHoldsIncompleteOpen(t *testing.T) {
	var f ThinkTagStreamFilter

	resp, think := f.Process("prefix <thi")
	require.Equal(t, "prefix ", resp)
	require.Empty(t, think)

	resp, think = f.Process("nk>hidden")
	require.Empty(t, resp)
	require.Equal(t, "hidden", think)
}

func TestThinkTagStreamFilterHoldsIncompleteClose(t *testing.T) {
	var f ThinkTagStreamFilter

	_, think := f.Process("<think>alpha ")
	require.Equal(t, "alpha ", think)

	_, think = f.Process("beta</thi")
	require.Equal(t, "beta", think)

	resp, think := f.Process("nk> tail")
	require.Equal(t, " tail", resp)
	require.Empty(t, think)
}

func TestExtractThinkTagsThink(t *testing.T) {
	think, resp := ExtractThinkTags("before<think> alpha </think> after")
	require.Equal(t, "alpha", think)
	require.Equal(t, "before after", resp)
}

func TestThinkTagStreamFilterFlush(t *testing.T) {
	var f ThinkTagStreamFilter

	resp, think := f.Process("answer<think>tail")
	require.Equal(t, "answer", resp)
	require.Equal(t, "tail", think)

	resp, think = f.Flush("")
	require.Empty(t, resp)
	require.Empty(t, think)
}