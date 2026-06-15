package renderer

import (
	"strings"
	"time"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/align"
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

// contentVisibleBodyLines is how many content lines fit below an active sticky header.
func (m Model) contentVisibleBodyLines(scrollTop int) int {
	vpH := m.content.Height()
	_, stickyH := m.stickyUserAtScrollTop(scrollTop)
	visible := vpH - stickyH
	if visible < 1 {
		return 1
	}
	return visible
}

// contentScrollTopForBottom is the scroll offset that shows the last content line
// when a sticky user header consumes part of the viewport height.
func (m Model) contentScrollTopForBottom() int {
	total := m.content.TotalLineCount()
	if total <= 0 {
		return 0
	}
	vpH := m.content.Height()
	if total <= vpH {
		return 0
	}

	scrollTop := total - vpH
	for range 8 {
		visible := m.contentVisibleBodyLines(scrollTop)
		newTop := total - visible
		if newTop < 0 {
			newTop = 0
		}
		if newTop == scrollTop {
			break
		}
		scrollTop = newTop
	}
	return scrollTop
}

// contentBodyScrollTop is the line index used to paint the scrollable body. When
// the viewport is at its max offset but a sticky header is active, the body must
// start lower than YOffset so the bottom lines are not clipped.
func (m Model) contentBodyScrollTop() int {
	scrollTop := m.content.YOffset()
	if !m.contentScrollable() {
		return scrollTop
	}
	standardMax := m.content.TotalLineCount() - m.content.Height()
	if standardMax < 0 {
		standardMax = 0
	}
	if scrollTop < standardMax {
		return scrollTop
	}
	return m.contentScrollTopForBottom()
}

// contentLineAtViewportY maps a viewport-local Y to a line index in the full
// scrollable content, accounting for the sticky header inset.
func (m Model) contentLineAtViewportY(y int) (int, bool) {
	displayTop := m.contentBodyScrollTop()
	_, stickyH := m.stickyUserAtScrollTop(displayTop)
	if y < stickyH {
		return -1, false
	}
	return y - stickyH + displayTop, true
}

// viewportYForContentLine maps a full-content line index to viewport-local Y.
func (m Model) viewportYForContentLine(contentLine int) (int, bool) {
	displayTop := m.contentBodyScrollTop()
	_, stickyH := m.stickyUserAtScrollTop(displayTop)
	y := contentLine - displayTop + stickyH
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

func userMessageStickyTitle(text string, maxW int) string {
	if maxW <= 0 {
		return ""
	}
	title := ""
	if userMessageCollapsible(text) {
		title = userMessageBody(text, false, maxW)
	} else {
		title = ansi.Truncate(strings.TrimSpace(text), maxW, "...")
	}
	if title == "" {
		return ""
	}
	return constants.StickyUserTitleStyle().Render(title)
}

func userMessageStickyLine(text string, innerW int, at time.Time) string {
	ts := formatMessageTimestamp(at)
	if ts == "" {
		return userMessageStickyTitle(text, innerW)
	}

	right := constants.StickyUserTimestampStyle().Render(ts)
	rightW := lipgloss.Width(right)
	leftColW := max(innerW-rightW, 1)
	titleW := max(leftColW-align.ColumnGap, 1)
	left := userMessageStickyTitle(text, titleW)
	return stickyUserFooterRow(innerW, left, right)
}

// stickyUserFooterRow lays out the sticky title and timestamp on one line. The left
// column is width-fixed with UserStickyMsgBg so padded gap before the timestamp
// matches the sticky header backdrop.
func stickyUserFooterRow(contentW int, left, right string) string {
	rightW := lipgloss.Width(right)
	if rightW >= contentW {
		return stickyUserLineStyle(contentW).Render(clampLine(contentW, right))
	}
	leftW := max(contentW-rightW, 0)
	leftPart := stickyUserLineStyle(leftW).Render(left)
	row := lipgloss.JoinHorizontal(lipgloss.Top, leftPart, right)
	return stickyUserLineStyle(contentW).Render(row)
}

func stickyUserLineStyle(width int) lipgloss.Style {
	return lipgloss.NewStyle().
		Background(constants.UserStickyMsgBg).
		Width(width).
		MaxHeight(1)
}

func renderUserSticky(blockWidth int, text string, at time.Time) string {
	vPad, hPad := messageBlockPadding(constants.MessageUser)
	style := constants.StickyUserStyle()
	innerW := userBoxInnerWidth(blockWidth, hPad)

	content := userMessageStickyLine(text, innerW, at)
	return renderUserBoxWithLeftBar(
		blockWidth,
		constants.UserStickyMsgBg,
		constants.UserStickyTimestampFg,
		style,
		vPad,
		hPad,
		content,
	)
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
	scrollTop := m.contentBodyScrollTop()
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
