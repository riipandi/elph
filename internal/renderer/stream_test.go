package renderer

import (
	"strings"
	"testing"
	"time"

	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func TestStreamFlushThrottlesLayoutRebuild(t *testing.T) {
	m := testModel()
	m.ready = true
	m.height = 24
	m.agent.Busy = true
	m.messages = []message{{text: "seed", kind: uiconst.MessageAI}}
	m.agent.ResponseMsgID = 0

	updated, cmd := m.markStreamDirty()
	require.NotNil(t, cmd)
	require.True(t, updated.layout.StreamFlushPending)

	updated.messages[0].text = "seed tokens"
	updated, _ = updated.markStreamDirty()
	require.True(t, updated.layout.StreamFlushPending, "second delta should not schedule another tick immediately")

	flushed, _ := updated.handleStreamFlush()
	require.False(t, flushed.layout.StreamFlushPending)
	require.False(t, flushed.layout.ContentDirty)
}

func TestStreamPrefixCacheReusesStableHead(t *testing.T) {
	m := testModel()
	m.width = 80
	m.content.SetWidth(80)
	m.agent.Busy = true
	m.messages = []message{
		{text: "user prompt", kind: uiconst.MessageUser},
		{text: "thinking", kind: uiconst.MessageThinking, detailLabel: "Thinking"},
		{text: "partial", kind: uiconst.MessageAI},
	}
	m.agent.ThinkingMsgID = 1
	m.agent.ResponseMsgID = 2

	m = m.refreshStreamPrefixCache()
	require.Equal(t, 1, m.layout.StreamPrefixUpTo, "in-flight thinking must stay out of frozen prefix")
	prefix := m.layout.StreamPrefix

	m.messages[2].text = "partial response"
	m.messages[2].renderCache = messageRenderCache{}
	full := m.messagesView()
	require.True(t, strings.HasPrefix(full, prefix))
}

func TestThinkingDetailBoxUpdatesDuringResponseStream(t *testing.T) {
	m := testInputModel(t)
	m.height = 24
	m.ready = true
	m.messages = []message{{text: "prompt", kind: uiconst.MessageUser}}
	m = m.beginAgentTurn()
	m = m.addThinkingMessage("")
	m.agent.ThinkingMsgID = 1
	m.agent.Busy = true

	view := stripANSI(m.messagesView())
	require.Contains(t, view, "Thinking")

	m.messages[1].text = "reasoning alpha"
	m.messages[1].renderCache = messageRenderCache{}
	m = m.clearStreamPrefixCache()
	m.messages = append(m.messages, message{text: "answer", kind: uiconst.MessageAI})
	m.agent.ResponseMsgID = 2

	m = m.refreshStreamPrefixCache()
	view = stripANSI(m.messagesView())
	require.Contains(t, view, "Thinking")
	require.Contains(t, view, "reasoning alpha")
	require.Contains(t, view, "answer")

	m.messages[1].text = "reasoning alpha beta"
	m.messages[1].renderCache = messageRenderCache{}
	m = m.clearStreamPrefixCache()
	flushed, _ := m.handleStreamFlush()
	view = stripANSI(flushed.messagesView())
	require.Contains(t, view, "reasoning alpha beta")
	require.Contains(t, view, "Thinking")
}

func TestStreamingUsesSinglePassRender(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.messages = []message{{text: strings.Repeat("word ", 200), kind: uiconst.MessageAI}}
	m.agent.ResponseMsgID = 0

	start := time.Now()
	_ = m.renderMessageAt(0)
	require.Less(t, time.Since(start), 50*time.Millisecond)
}
