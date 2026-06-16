package renderer

import (
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/riipandi/elph/pkg/core/agent"
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

// streamPrefixEndIndex is the first message index excluded from the frozen
// stream prefix. In-flight thinking is always repainted so its detail box and
// live reasoning text stay current while the response streams.
func (m Model) streamPrefixEndIndex() int {
	streamIdx := m.streamingMessageIndex()
	if streamIdx < 0 {
		return -1
	}
	thinkIdx := m.agent.ThinkingMsgID
	if thinkIdx >= 0 && thinkIdx < streamIdx && m.thinkingInFlight(thinkIdx) {
		return thinkIdx
	}
	return streamIdx
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
	m.layout.StreamPrefixDetailSig = 0
	return m
}

func (m Model) streamPrefixDetailSig(streamIdx int) uint64 {
	var sig uint64
	for i := 0; i < streamIdx && i < len(m.messages); i++ {
		if !isCollapsibleKind(m.messages[i].kind) {
			continue
		}
		part := uint64(i+1) | uint64(m.messages[i].detailStatus)<<8
		if m.messages[i].detailExpanded {
			part |= 1 << 32
		}
		sig ^= part * 0x9e3779b97f4a7c15
	}
	return sig
}

func (m Model) refreshStreamPrefixCache() Model {
	prefixEnd := m.streamPrefixEndIndex()
	if prefixEnd < 0 {
		return m.clearStreamPrefixCache()
	}

	width := m.messageAreaWidth()
	beforeLen := m.messagesBeforeStreamLen(prefixEnd)
	detailSig := m.streamPrefixDetailSig(prefixEnd)
	if m.layout.StreamPrefixUpTo == prefixEnd &&
		m.layout.StreamPrefixWidth == width &&
		m.layout.StreamPrefixBeforeLen == beforeLen &&
		m.layout.StreamPrefixDetailSig == detailSig {
		return m
	}

	var b strings.Builder
	for i := 0; i < prefixEnd; i++ {
		if i > 0 {
			b.WriteString(messageBlockGap)
		}
		b.WriteString(m.renderMessageAt(i))
	}
	m.layout.StreamPrefix = b.String()
	m.layout.StreamPrefixUpTo = prefixEnd
	m.layout.StreamPrefixWidth = width
	m.layout.StreamPrefixBeforeLen = beforeLen
	m.layout.StreamPrefixDetailSig = detailSig
	return m
}

func (m Model) markStreamDirty() (Model, tea.Cmd) {
	m.layout.ContentDirty = true
	var cmds []tea.Cmd
	if !m.layout.StreamFlushPending {
		m.layout.StreamFlushPending = true
		cmds = append(cmds, streamFlushTick())
	}
	return m.batchAgentDrain(cmds...)
}

// flushContentNow repaints scrollable content immediately instead of waiting for
// the throttled stream flush tick.
func (m Model) flushContentNow() (Model, tea.Cmd) {
	m.layout.ContentDirty = true
	m = m.refreshStreamPrefixCache()
	m = m.syncLayout(m.content.AtBottom())
	return m.batchAgentDrain()
}

// flushThinkingStreamNow repaints the thinking detail box immediately instead of
// waiting for the throttled stream flush tick.
func (m Model) flushThinkingStreamNow() (Model, tea.Cmd) {
	return m.flushContentNow()
}

func (m Model) batchAgentDrain(cmds ...tea.Cmd) (Model, tea.Cmd) {
	cmds = append(cmds, m.drainAgentEvents()...)
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}

// drainAgentEvents schedules channel reads while a turn is active so provider
// stream callbacks never block on a full event buffer.
func (m Model) drainAgentEvents() []tea.Cmd {
	if m.agent.Events == nil {
		return nil
	}
	return []tea.Cmd{waitAgentEvent(m.agent.Events)}
}

func (m Model) handleStreamFlush() (Model, tea.Cmd) {
	m.layout.StreamFlushPending = false
	if !m.layout.ContentDirty {
		return m, nil
	}

	m = m.refreshStreamPrefixCache()
	m = m.syncLayout(m.content.AtBottom())

	// Re-arm the flush tick while either agent or shell is actively streaming.
	var cmds []tea.Cmd
	if m.layout.ContentDirty && (m.agent.Busy || m.shell.Running) {
		m.layout.StreamFlushPending = true
		cmds = append(cmds, streamFlushTick())
	}
	cmds = append(cmds, m.drainAgentEvents()...)
	if len(cmds) == 0 {
		return m, nil
	}
	return m, tea.Batch(cmds...)
}

// renderStreamingMessage paints an in-flight message in one pass. Avoids
// per-line Lip Gloss work on every token while the response grows.
func renderStreamingMessage(blockWidth int, kind uiconst.MessageKind, text string) string {
	if kind == uiconst.MessageAI || kind == uiconst.MessageThinking {
		text = agent.SanitizeAssistantDisplay(text)
	}
	if kind == uiconst.MessageAI {
		return renderAIMessage(blockWidth, text, true, false)
	}
	vPad, hPad := messageBlockPadding(kind)
	return uiconst.MessageStyle(kind).
		Padding(vPad, hPad).
		Width(blockWidth).
		Render(text)
}
