package renderer

import (
	"strings"
	"testing"
	"time"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestStreamFlushThrottlesLayoutRebuild(t *testing.T) {
	m := testModel()
	m.ready = true
	m.height = 24
	m.agent.Busy = true
	m.messages = []message{{text: "seed", kind: constants.MessageAI}}
	m.agent.ResponseMsgID = 0

	updated, cmd := m.markStreamDirty()
	require.NotNil(t, cmd)
	require.True(t, updated.layout.StreamFlushPending)

	updated.messages[0].text = "seed tokens"
	updated, cmd = updated.markStreamDirty()
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
		{text: "user prompt", kind: constants.MessageUser},
		{text: "thinking", kind: constants.MessageThinking},
		{text: "partial", kind: constants.MessageAI},
	}
	m.agent.ResponseMsgID = 2

	m = m.refreshStreamPrefixCache()
	require.Equal(t, 2, m.layout.StreamPrefixUpTo)
	prefix := m.layout.StreamPrefix

	m.messages[2].text = "partial response"
	m.messages[2].renderCache = messageRenderCache{}
	full := m.messagesView()
	require.True(t, strings.HasPrefix(full, prefix))
}

func TestStreamingUsesSinglePassRender(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.messages = []message{{text: strings.Repeat("word ", 200), kind: constants.MessageAI}}
	m.agent.ResponseMsgID = 0

	start := time.Now()
	_ = m.renderMessageAt(0)
	require.Less(t, time.Since(start), 50*time.Millisecond)
}
