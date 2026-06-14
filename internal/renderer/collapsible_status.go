package renderer

import "github.com/riipandi/elph/internal/constants"

func (m Model) collapsibleShowsStatusPreview(msg message, index int) bool {
	if msg.detailExpanded {
		return false
	}
	switch msg.kind {
	case constants.MessageThinking:
		return m.isStreamingMessageAt(index)
	case constants.MessageDetail:
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
	if !show {
		return collapsibleRenderOpts{}
	}
	return collapsibleRenderOpts{
		showStatusPreview: true,
		spinnerFrame:      m.agent.SpinnerFrame,
	}
}

func (m Model) needsSpinnerContentRefresh() bool {
	if !m.showsActivity() {
		return false
	}
	for i, msg := range m.messages {
		if m.collapsibleShowsStatusPreview(msg, i) {
			return true
		}
	}
	return false
}

func (m Model) invalidateSpinnerPreviewCaches() Model {
	for i, msg := range m.messages {
		if m.collapsibleShowsStatusPreview(msg, i) {
			m.messages[i].renderCache = messageRenderCache{}
		}
	}
	return m.clearStreamPrefixCache()
}
