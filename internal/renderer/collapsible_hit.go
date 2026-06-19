package renderer

import (
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/uiconst"
)

type contentLineZone int

const (
	zoneBanner contentLineZone = iota
	zoneGap
	zoneBody
	zoneCollapsibleHeader
	zoneCollapsibleFooter
	zoneAICopyFooter
)

type contentLineRef struct {
	messageIndex int
	zone         contentLineZone
}

func (m Model) walkContentLines(fn func(line int, ref contentLineRef) bool) {
	line := 0

	bannerH := lipgloss.Height(m.bannerView())
	for range bannerH {
		if fn(line, contentLineRef{messageIndex: -1, zone: zoneBanner}) {
			return
		}
		line++
	}

	if len(m.messages) == 0 {
		return
	}

	if fn(line, contentLineRef{messageIndex: -1, zone: zoneGap}) {
		return
	}
	line++

	for i := range m.messages {
		if i > 0 {
			if fn(line, contentLineRef{messageIndex: -1, zone: zoneGap}) {
				return
			}
			line++
		}

		msg := m.messages[i]
		rendered := m.renderMessageAt(i)
		rows := strings.Split(rendered, "\n")
		blockH := len(rows)
		copyFooterRow := aiCopyHintRow(rows, msg, m.isStreamingMessageAt(i))

		headerRow, footerRow := collapsibleToggleRows(msg, rows, blockH)

		for row := range blockH {
			ref := contentLineRef{messageIndex: i, zone: zoneBody}
			switch {
			case headerRow >= 0 && row == headerRow:
				ref.zone = zoneCollapsibleHeader
			case footerRow >= 0 && row == footerRow:
				ref.zone = zoneCollapsibleFooter
			case copyFooterRow >= 0 && row == copyFooterRow:
				ref.zone = zoneAICopyFooter
			}
			if fn(line, ref) {
				return
			}
			line++
		}
	}
}

func collapsibleToggleRows(msg message, rows []string, blockH int) (headerRow, footerRow int) {
	if !messageCollapsible(msg) {
		return -1, -1
	}
	switch msg.kind {
	case uiconst.MessageUser:
		for i, row := range rows {
			if rowContainsCollapsibleHint(row) {
				footerRow = i
				break
			}
		}
		for i, row := range rows {
			plain := strings.TrimSpace(ansi.Strip(row))
			if plain == "" || rowContainsCollapsibleHint(row) {
				continue
			}
			headerRow = i
			break
		}
	default:
		headerRow = 0
		for i, row := range rows {
			if rowContainsCollapsibleHint(row) {
				footerRow = i
				break
			}
		}
		// If no hint found (e.g. running status), fall back to last row
		if footerRow < 0 {
			footerRow = blockH - 1
		}
	}
	return headerRow, footerRow
}

func (m Model) collapsibleToggleAtViewportY(y int) (int, bool) {
	if !m.isInContentArea(y) {
		return -1, false
	}
	if stickyIdx := m.stickyUserMessageIndex(m.content.YOffset()); stickyIdx >= 0 {
		if y < m.stickyUserOverlayHeight(stickyIdx) {
			return stickyIdx, true
		}
	}
	contentLine, ok := m.contentLineAtViewportY(y)
	if !ok {
		return -1, false
	}
	found := -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if line != contentLine {
			return false
		}
		switch ref.zone {
		case zoneCollapsibleFooter:
			found = ref.messageIndex
		case zoneCollapsibleHeader:
			if ref.messageIndex >= 0 && ref.messageIndex < len(m.messages) {
				msg := m.messages[ref.messageIndex]
				switch msg.kind {
				case uiconst.MessageThinking:
					found = ref.messageIndex
				case uiconst.MessageUser:
					if userMessageCollapsible(msg.text) {
						found = ref.messageIndex
					}
				}
			}
		}
		return true
	})
	if found < 0 {
		return -1, false
	}
	return found, true
}

func (m Model) collapsibleFooterViewportY(msgIndex int) (int, bool) {
	target := -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if ref.messageIndex == msgIndex && ref.zone == zoneCollapsibleFooter {
			target = line
			return true
		}
		return false
	})
	if target < 0 {
		return -1, false
	}
	return m.viewportYForContentLine(target)
}

func aiCopyHintRow(rows []string, msg message, streaming bool) int {
	if streaming {
		return -1
	}
	if msg.kind != uiconst.MessageAI && !msg.copyable {
		return -1
	}
	for i, row := range rows {
		if strings.Contains(ansi.Strip(row), aiCopyHintText) {
			return i
		}
	}
	return -1
}

func (m Model) collapsibleHeaderViewportY(msgIndex int) (int, bool) {
	target := -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if ref.messageIndex == msgIndex && ref.zone == zoneCollapsibleHeader {
			target = line
			return true
		}
		return false
	})
	if target < 0 {
		return -1, false
	}
	return m.viewportYForContentLine(target)
}
