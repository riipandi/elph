package renderer

import (
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/constants"
)

const streamFlushInterval = 40 * time.Millisecond

type streamFlushMsg struct{}

func streamFlushTick() tea.Cmd {
	return tea.Tick(streamFlushInterval, func(time.Time) tea.Msg {
		return streamFlushMsg{}
	})
}

func (m Model) streamingMessageIndex() int {
	if !m.agent.Busy {
		return -1
	}
	if m.agent.ResponseMsgID >= 0 && m.agent.ResponseMsgID < len(m.messages) {
		return m.agent.ResponseMsgID
	}
	if m.showThinkingEnabled() && m.agent.ThinkingMsgID >= 0 && m.agent.ThinkingMsgID < len(m.messages) {
		return m.agent.ThinkingMsgID
	}
	return -1
}

func (m Model) isStreamingMessageAt(index int) bool {
	return index >= 0 && index == m.streamingMessageIndex()
}

func (m Model) messagesBeforeStreamLen(streamIdx int) int {
	var n int
	for i := 0; i < streamIdx && i < len(m.messages); i++ {
		n += len(m.messages[i].text)
	}
	return n
}

func (m Model) clearStreamPrefixCache() Model {
	m.layout.StreamPrefix = ""
	m.layout.StreamPrefixUpTo = -1
	m.layout.StreamPrefixWidth = 0
	m.layout.StreamPrefixBeforeLen = 0
	return m
}

func (m Model) refreshStreamPrefixCache() Model {
	streamIdx := m.streamingMessageIndex()
	if streamIdx <= 0 {
		return m.clearStreamPrefixCache()
	}

	width := m.messageAreaWidth()
	beforeLen := m.messagesBeforeStreamLen(streamIdx)
	if m.layout.StreamPrefixUpTo == streamIdx &&
		m.layout.StreamPrefixWidth == width &&
		m.layout.StreamPrefixBeforeLen == beforeLen {
		return m
	}

	var b strings.Builder
	for i := 0; i < streamIdx; i++ {
		if i > 0 {
			b.WriteString(messageBlockGap)
		}
		b.WriteString(m.renderMessageAt(i))
	}
	m.layout.StreamPrefix = b.String()
	m.layout.StreamPrefixUpTo = streamIdx
	m.layout.StreamPrefixWidth = width
	m.layout.StreamPrefixBeforeLen = beforeLen
	return m
}

func (m Model) markStreamDirty() (Model, tea.Cmd) {
	m.layout.ContentDirty = true
	var cmds []tea.Cmd
	if !m.layout.StreamFlushPending {
		m.layout.StreamFlushPending = true
		cmds = append(cmds, streamFlushTick())
	}
	if m.agent.Events != nil {
		cmds = append(cmds, waitAgentEvent(m.agent.Events))
	}
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}

func (m Model) handleStreamFlush() (Model, tea.Cmd) {
	m.layout.StreamFlushPending = false
	if !m.layout.ContentDirty {
		return m, nil
	}

	m = m.refreshStreamPrefixCache()
	m = m.syncLayout(m.content.AtBottom())

	var cmds []tea.Cmd
	if m.layout.ContentDirty && m.agent.Busy {
		m.layout.StreamFlushPending = true
		cmds = append(cmds, streamFlushTick())
	}
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}

// renderStreamingMessage paints an in-flight message in one pass. Avoids
// per-line Lip Gloss work on every token while the response grows.
func renderStreamingMessage(blockWidth int, kind constants.MessageKind, text string) string {
	vPad, hPad := messageBlockPadding(kind)
	return constants.MessageStyle(kind).
		Padding(vPad, hPad).
		Width(blockWidth).
		Render(text)
}
