package renderer

import (
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/constants"
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
		if fn(line, contentLineRef{zone: zoneBanner}) {
			return
		}
		line++
	}

	if len(m.messages) == 0 {
		return
	}

	if fn(line, contentLineRef{zone: zoneGap}) {
		return
	}
	line++

	for i := range m.messages {
		if i > 0 {
			if fn(line, contentLineRef{zone: zoneGap}) {
				return
			}
			line++
		}

		msg := m.messages[i]
		rendered := m.renderMessageAt(i)
		rows := strings.Split(rendered, "\n")
		blockH := len(rows)
		copyFooterRow := aiCopyHintRow(rows, msg, m.isStreamingMessageAt(i))

		for row := range blockH {
			ref := contentLineRef{messageIndex: i, zone: zoneBody}
			switch {
			case isCollapsibleKind(msg.kind):
				switch row {
				case 0:
					ref.zone = zoneCollapsibleHeader
				case blockH - 1:
					ref.zone = zoneCollapsibleFooter
				}
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

func (m Model) collapsibleToggleAtViewportY(y int) (int, bool) {
	if !m.isInContentArea(y) {
		return -1, false
	}
	contentLine := y + m.content.YOffset()
	var found = -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if line != contentLine {
			return false
		}
		switch ref.zone {
		case zoneCollapsibleFooter:
			found = ref.messageIndex
		case zoneCollapsibleHeader:
			if ref.messageIndex >= 0 &&
				ref.messageIndex < len(m.messages) &&
				m.messages[ref.messageIndex].kind == constants.MessageThinking {
				found = ref.messageIndex
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
	y := target - m.content.YOffset()
	return y, y >= 0 && y < m.content.Height()
}

func aiCopyHintRow(rows []string, msg message, streaming bool) int {
	if msg.kind != constants.MessageAI || streaming {
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
	y := target - m.content.YOffset()
	return y, y >= 0 && y < m.content.Height()
}
