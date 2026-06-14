package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/riipandi/elph/pkg/core/agent"
	"github.com/stretchr/testify/require"
)

func TestAgentTurnCreatesThinkingPlaceholder(t *testing.T) {
	m := testInputModel(t)
	m.messages = []message{{text: "hello", kind: constants.MessageUser}}
	m = m.beginAgentTurn()

	m, cmd := m.agentTurnCmds("hello")
	require.NotNil(t, cmd)
	require.Len(t, m.messages, 2)
	require.Equal(t, constants.MessageThinking, m.messages[1].kind)
	require.Equal(t, 1, m.agent.ThinkingMsgID)

	view := stripANSI(m.messagesView())
	require.Contains(t, view, "Thinking")
	require.Contains(t, view, "Thinking...")
}

func TestThinkingPlaceholderStreamsBeforeResponse(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	updated, _ := m.Update(agentEventMsg{event: agent.ThinkingDeltaEvent("step one")})
	m = updated.(Model)
	updated, _ = m.Update(agentEventMsg{event: agent.ResponseDeltaEvent("answer")})
	m = updated.(Model)
	flushed, _ := m.handleStreamFlush()
	m = flushed

	view := stripANSI(m.messagesView())
	require.Contains(t, view, "Thinking")
	require.Contains(t, view, "step one")
	require.Contains(t, view, "answer")
	require.Less(t, stringsIndex(view, "Thinking"), stringsIndex(view, "answer"))
}

func TestThinkTagsRouteToThinkingBoxDuringResponseStream(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	updated, _ := m.Update(agentEventMsg{event: agent.ResponseDeltaEvent("<think>reason step</think>visible")})
	m = updated.(Model)
	flushed, _ := m.handleStreamFlush()
	m = flushed

	require.Equal(t, "reason step", m.messages[1].text)
	require.Equal(t, "visible", m.messages[2].text)

	view := stripANSI(m.messagesView())
	require.Contains(t, view, "reason step")
	require.Contains(t, view, "visible")
	require.NotContains(t, view, "<think>")
}

func TestManyThinkingDeltasDoNotStallUpdateLoop(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	for range 200 {
		updated, _ := m.Update(agentEventMsg{event: agent.ThinkingDeltaEvent("x")})
		m = updated.(Model)
	}
	m, _ = m.handleStreamFlush()

	require.Greater(t, len(m.messages[1].text), 100)
	view := stripANSI(m.contentView())
	require.Contains(t, view, "xxx")
}

func TestThinkingDeltaRepaintsImmediatelyWithoutFlushTick(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	updated, _ := m.Update(agentEventMsg{event: agent.ThinkingDeltaEvent("live reasoning")})
	m = updated.(Model)

	view := stripANSI(m.contentView())
	require.Contains(t, view, "Thinking")
	require.Contains(t, view, "live reasoning")
}

func TestThinkingDeltasRepaintViewportThroughUpdateLoop(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	for i := range 20 {
		updated, _ := m.Update(agentEventMsg{event: agent.ThinkingDeltaEvent("step ")})
		m = updated.(Model)
		require.Equal(t, i+1, len(m.messages[1].text)/len("step "), "thinking text should grow each delta")
	}
	m, _ = m.handleStreamFlush()

	view := stripANSI(m.contentView())
	require.Contains(t, view, "Thinking")
	require.Contains(t, view, "step step step")
}

func TestThinkTagsStreamIncrementallyIntoThinkingBox(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: constants.MessageUser}}
	m = m.beginAgentTurn()
	m, _ = m.agentTurnCmds("prompt")

	chunks := []string{"<thi", "nk>step ", "one", "</think>", "answer"}
	for _, chunk := range chunks {
		updated, _ := m.Update(agentEventMsg{event: agent.ResponseDeltaEvent(chunk)})
		m = updated.(Model)
	}
	flushed, _ := m.handleStreamFlush()
	m = flushed

	require.Equal(t, "step one", m.messages[1].text)
	view := stripANSI(m.messagesView())
	require.Contains(t, view, "step one")
}

func stringsIndex(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}