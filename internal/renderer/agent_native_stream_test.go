package renderer

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestBeginNativeToolCallBashShowsCommandLabel(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{
		ID:        "call_bash",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"ping 1.1.1.1"}`),
	}
	m = m.beginNativeToolCall(call)
	idx := m.agent.NativeToolMsgIDs["call_bash"]
	require.Equal(t, "$ ping 1.1.1.1", m.messages[idx].detailLabel)
}

func TestAppendNativeToolOutputStreamsIntoDetailBox(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{
		ID:        "call_stream",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"echo hi"}`),
	}
	m = m.beginNativeToolCall(call)

	m = m.appendNativeToolOutput(call, "he")
	m = m.appendNativeToolOutput(call, "llo\n")

	idx := m.agent.NativeToolMsgIDs["call_stream"]
	require.Equal(t, "hello", strings.TrimRight(m.messages[idx].text, "\n"))

	view := stripANSI(m.renderMessageAt(idx))
	require.Contains(t, view, "hello")
	require.NotContains(t, view, "Running...")
}

func TestAppendNativeToolOutputHonorsCarriageReturn(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{
		ID:        "call_ping",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"ping 1.1.1.1"}`),
	}
	m = m.beginNativeToolCall(call)

	m = m.appendNativeToolOutput(call, "PING 1.1.1.1\n64 bytes")
	m = m.appendNativeToolOutput(call, "\r128 bytes")

	idx := m.agent.NativeToolMsgIDs["call_ping"]
	require.Equal(t, "PING 1.1.1.1\n128 bytes", m.messages[idx].text)
}

func TestAppendNativeToolOutputPreservesChunkBoundaryNewlines(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{
		ID:        "call_ping_chunks",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"ping 1.1.1.1"}`),
	}
	m = m.beginNativeToolCall(call)

	m = m.appendNativeToolOutput(call, "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n64 bytes from 1.1.1.1: icmp_seq=0 ttl=58 time=9.046 ms\n")
	m = m.appendNativeToolOutput(call, "64 bytes from 1.1.1.1: icmp_seq=1 ttl=58 time=9.158 ms\n")

	idx := m.agent.NativeToolMsgIDs["call_ping_chunks"]
	require.Contains(t, m.messages[idx].text, "ms\n64 bytes")
	require.NotContains(t, m.messages[idx].text, "ms64 bytes")
}

func TestFinishNativeToolCallBashKeepsRawStreamedOutput(t *testing.T) {
	m := testInputModel(t)
	call := provider.ToolCall{
		ID:        "call_bash_done",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"false"}`),
	}
	m = m.beginNativeToolCall(call)
	m = m.appendNativeToolOutput(call, "failed\n")

	m = m.finishNativeToolCall(call, agent.ToolRunResult{Output: "failed\n\n(exit 1)"})

	idx := m.agent.NativeToolMsgIDs["call_bash_done"]
	require.Equal(t, "$ false", m.messages[idx].detailLabel)
	require.Equal(t, "failed\n(exit 1)", m.messages[idx].text)
	require.NotContains(t, m.messages[idx].text, "Tool failed")
}

func TestFinishNativeToolCallAllCollapsedByDefault(t *testing.T) {
	m := testInputModel(t)

	readCall := provider.ToolCall{
		ID:        "call_read",
		Name:      "Read",
		Arguments: json.RawMessage(`{"path":"big.txt"}`),
	}
	m = m.beginNativeToolCall(readCall)
	m = m.finishNativeToolCall(readCall, agent.ToolRunResult{Output: "alpha\nbeta\ngamma"})
	readIdx := m.agent.NativeToolMsgIDs["call_read"]
	require.False(t, m.messages[readIdx].detailExpanded)

	bashCall := provider.ToolCall{
		ID:        "call_bash_long",
		Name:      "Bash",
		Arguments: json.RawMessage(`{"command":"seq 3"}`),
	}
	m = m.beginNativeToolCall(bashCall)
	m = m.finishNativeToolCall(bashCall, agent.ToolRunResult{Output: "1\n2\n3"})
	bashIdx := m.agent.NativeToolMsgIDs["call_bash_long"]
	require.False(t, m.messages[bashIdx].detailExpanded, "Bash tool detail should also collapse by default")
}
