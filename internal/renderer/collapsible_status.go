package renderer

import (
	"strings"

	"github.com/riipandi/elph/internal/uiconst"
)

func (m Model) thinkingInFlight(index int) bool {
	return m.agent.Busy && index == m.agent.ThinkingMsgID
}

func (m Model) thinkingShowsLiveBody(msg message, index int) bool {
	if msg.kind != uiconst.MessageThinking {
		return false
	}
	if !msg.detailExpanded {
		return false
	}
	if strings.TrimSpace(msg.text) == "" {
		return false
	}
	// Keep the body live for the whole turn once reasoning text exists.
	return m.agent.Busy && index == m.agent.ThinkingMsgID
}

func (m Model) collapsibleShowsStatusPreview(msg message, index int) bool {
	switch msg.kind {
	case uiconst.MessageThinking:
		if strings.TrimSpace(msg.text) != "" {
			return false
		}
		return m.thinkingInFlight(index)
	case uiconst.MessageDetail:
		if msg.detailStatus == uiconst.DetailStatusRunning && isRunningDetailPlaceholder(msg.text) {
			return true
		}
		if msg.detailExpanded {
			return false
		}
		switch msg.detailStatus {
		case uiconst.DetailStatusRunning:
			return true
		case uiconst.DetailStatusError, uiconst.DetailStatusWarning, uiconst.DetailStatusUnavailable:
			return true
		default:
			return false
		}
	default:
		return false
	}
}

func (m Model) collapsibleRenderOpts(msg message, index int) collapsibleRenderOpts {
	show := m.collapsibleShowsStatusPreview(msg, index)
	live := m.thinkingShowsLiveBody(msg, index) || m.nativeToolOutputStreaming(index)
	return collapsibleRenderOpts{
		showStatusPreview: show,
		showLiveBody:      live,
		spinnerFrame:      m.agent.SpinnerFrame,
		copyable:          msg.copyable,
	}
}

func (m Model) nativeToolOutputStreaming(index int) bool {
	if index < 0 || index >= len(m.messages) {
		return false
	}
	msg := m.messages[index]
	if msg.kind != uiconst.MessageDetail || msg.detailStatus != uiconst.DetailStatusRunning {
		return false
	}
	return !isRunningDetailPlaceholder(msg.text)
}

func (m Model) collapsibleNeedsLiveRefresh(msg message, index int) bool {
	return m.collapsibleShowsStatusPreview(msg, index) ||
		m.thinkingShowsLiveBody(msg, index) ||
		m.nativeToolOutputStreaming(index)
}

func (m Model) needsSpinnerContentRefresh() bool {
	if m.agent.TodoListUpdating {
		return true
	}
	if !m.showsActivity() {
		return false
	}
	for i, msg := range m.messages {
		if m.collapsibleNeedsLiveRefresh(msg, i) {
			return true
		}
	}
	return false
}

func (m Model) invalidateSpinnerPreviewCaches() Model {
	for i, msg := range m.messages {
		if m.collapsibleNeedsLiveRefresh(msg, i) {
			m.messages[i].renderCache = messageRenderCache{}
		}
	}
	return m.clearStreamPrefixCache()
}
