package renderer

import (
	"strings"

	"github.com/riipandi/elph/internal/constants"
)

func (m Model) thinkingInFlight(index int) bool {
	return m.agent.Busy && index == m.agent.ThinkingMsgID
}

func (m Model) thinkingShowsLiveBody(msg message, index int) bool {
	if msg.kind != constants.MessageThinking {
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
	case constants.MessageThinking:
		if strings.TrimSpace(msg.text) != "" {
			return false
		}
		return m.thinkingInFlight(index)
	case constants.MessageDetail:
		if msg.detailExpanded {
			return false
		}
		switch msg.detailStatus {
		case constants.DetailStatusRunning:
			return true
		case constants.DetailStatusError, constants.DetailStatusWarning, constants.DetailStatusUnavailable:
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
	live := m.thinkingShowsLiveBody(msg, index)
	if !show && !live {
		return collapsibleRenderOpts{}
	}
	return collapsibleRenderOpts{
		showStatusPreview: show,
		showLiveBody:      live,
		spinnerFrame:      m.agent.SpinnerFrame,
	}
}

func (m Model) collapsibleNeedsLiveRefresh(msg message, index int) bool {
	return m.collapsibleShowsStatusPreview(msg, index) || m.thinkingShowsLiveBody(msg, index)
}

func (m Model) needsSpinnerContentRefresh() bool {
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
