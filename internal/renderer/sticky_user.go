package renderer

import (
	"strings"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
)

// messageBlockLineRange returns the [start, end) line span of a message block in
// the full scrollable content (banner + messages).
func (m Model) messageBlockLineRange(msgIndex int) (start, end int, ok bool) {
	found := false
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if ref.messageIndex != msgIndex {
			return false
		}
		if !found {
			start = line
			found = true
		}
		end = line + 1
		return false
	})
	return start, end, found
}

// userMessageScrollAnchor is the first line of a user message block — the line
// that should stick once it scrolls above the viewport top.
func (m Model) userMessageScrollAnchor(msgIndex int) (anchor int, ok bool) {
	anchor = -1
	m.walkContentLines(func(line int, ref contentLineRef) bool {
		if ref.messageIndex != msgIndex {
			return false
		}
		if ref.zone == zoneCollapsibleHeader {
			anchor = line
			return true
		}
		if anchor < 0 {
			anchor = line
		}
		return false
	})
	return anchor, anchor >= 0
}

func (m Model) nextUserMessageIndex(after int) int {
	for i := after + 1; i < len(m.messages); i++ {
		if m.messages[i].kind == constants.MessageUser {
			return i
		}
	}
	return -1
}

// stickyUserMessageIndex returns the user message that should pin to the top of
// the content viewport at the current scroll offset.
func (m Model) stickyUserMessageIndex(scrollTop int) int {
	if !m.stickyScroll || !m.contentScrollable() {
		return -1
	}

	candidate := -1
	for i, msg := range m.messages {
		if msg.kind != constants.MessageUser {
			continue
		}
		anchor, ok := m.userMessageScrollAnchor(i)
		if ok && anchor < scrollTop {
			candidate = i
		}
	}
	if candidate < 0 {
		return -1
	}

	if next := m.nextUserMessageIndex(candidate); next >= 0 {
		if anchor, ok := m.userMessageScrollAnchor(next); ok && anchor <= scrollTop {
			return -1
		}
	}
	return candidate
}

func (m Model) stickyUserAtScrollTop(scrollTop int) (msgIndex int, height int) {
	msgIndex = m.stickyUserMessageIndex(scrollTop)
	if msgIndex < 0 {
		return -1, 0
	}
	return msgIndex, lipgloss.Height(m.renderUserSticky(msgIndex))
}

func (m Model) stickyUserOverlayHeight(msgIndex int) int {
	if msgIndex < 0 {
		return 0
	}
	return lipgloss.Height(m.renderUserSticky(msgIndex))
}

// contentLineAtViewportY maps a viewport-local Y to a line index in the full
// scrollable content, accounting for the sticky header inset.
func (m Model) contentLineAtViewportY(y int) (int, bool) {
	_, stickyH := m.stickyUserAtScrollTop(m.content.YOffset())
	if y < stickyH {
		return -1, false
	}
	return y - stickyH + m.content.YOffset(), true
}

// viewportYForContentLine maps a full-content line index to viewport-local Y.
func (m Model) viewportYForContentLine(contentLine int) (int, bool) {
	_, stickyH := m.stickyUserAtScrollTop(m.content.YOffset())
	y := contentLine - m.content.YOffset() + stickyH
	return y, y >= stickyH && y < m.content.Height()
}

// renderUserSticky paints a compact collapsed user prompt for the sticky header.
func (m Model) renderUserSticky(msgIndex int) string {
	if msgIndex < 0 || msgIndex >= len(m.messages) {
		return ""
	}
	msg := m.messages[msgIndex]
	if msg.kind != constants.MessageUser {
		return ""
	}
	return renderUserSticky(m.messageAreaWidth(), msg.text, msg.at)
}

func renderUserSticky(blockWidth int, text string, at time.Time) string {
	vPad, hPad := messageBlockPadding(constants.MessageUser)
	style := constants.MessageStyle(constants.MessageUser)
	innerW := max(blockWidth-2*hPad, 1)

	body := userMessageBody(text, false, innerW)
	content := body
	if footer := userMessageFooterLine(at, false, false); footer != "" {
		content = body + "\n\n" + footer
	}
	return style.Padding(vPad, hPad).Width(blockWidth).Render(content)
}

func (m Model) sliceContentAt(yOffset, count int) string {
	if count <= 0 {
		return ""
	}
	content := m.content.GetContent()
	if content == "" {
		return ""
	}
	lines := strings.Split(content, "\n")
	if yOffset >= len(lines) {
		return strings.Repeat("\n", count-1)
	}
	end := min(yOffset+count, len(lines))
	slice := lines[yOffset:end]
	for len(slice) < count {
		slice = append(slice, "")
	}
	return strings.Join(slice, "\n")
}

func (m Model) contentBodyView() string {
	scrollTop := m.content.YOffset()
	stickyIdx, stickyH := m.stickyUserAtScrollTop(scrollTop)

	vpH := m.content.Height()
	sticky := ""
	if stickyIdx >= 0 {
		sticky = m.renderUserSticky(stickyIdx)
		vpH -= stickyH
		if vpH < 1 {
			vpH = 1
		}
	}

	vp := m.sliceContentAt(scrollTop, vpH)
	if sticky == "" {
		return vp
	}
	return lipgloss.JoinVertical(lipgloss.Top, sticky, vp)
}
