package renderer

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/align"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func stickyScrollTestModel(t *testing.T) Model {
	t.Helper()
	m := New()
	m.width = 80
	m.height = 12
	m.ready = true
	m.messages = []message{
		{
			text: "user prompt line one\nuser prompt line two",
			kind: uiconst.MessageUser,
			at:   time.Date(2026, 6, 15, 9, 0, 0, 0, time.Local),
		},
		{
			text: strings.Repeat("assistant response line\n", 40),
			kind: uiconst.MessageAI,
		},
	}
	m.layout.ContentDirty = true
	m.stickyScroll = true
	return m.syncLayout(false)
}

func TestStickyScrollDisabledBySetting(t *testing.T) {
	m := stickyScrollTestModel(t)
	m.stickyScroll = false
	_, userEnd, ok := m.messageBlockLineRange(0)
	require.True(t, ok)
	m.content.SetYOffset(userEnd + 1)

	require.Equal(t, -1, m.stickyUserMessageIndex(m.content.YOffset()))
	view := stripANSI(m.contentBodyView())
	require.NotContains(t, view, "▸")
}

func TestStickyUserMessageIndexWhenReadingAIResponse(t *testing.T) {
	m := stickyScrollTestModel(t)
	require.True(t, m.contentScrollable())

	_, userEnd, ok := m.messageBlockLineRange(0)
	require.True(t, ok)
	m.content.SetYOffset(userEnd + 5)

	require.Equal(t, 0, m.stickyUserMessageIndex(m.content.YOffset()))
}

func TestStickyUserMessageIndexHiddenAtTop(t *testing.T) {
	m := stickyScrollTestModel(t)
	m.content.GotoTop()
	require.Equal(t, -1, m.stickyUserMessageIndex(m.content.YOffset()))
}

func TestStickyUserMessageIndexHiddenWhenNextUserVisible(t *testing.T) {
	m := stickyScrollTestModel(t)
	m.messages = append(m.messages, message{
		text: "second user prompt",
		kind: uiconst.MessageUser,
		at:   time.Now(),
	}, message{
		text: strings.Repeat("second response\n", 10),
		kind: uiconst.MessageAI,
	})
	m.layout.ContentDirty = true
	m = m.syncLayout(false)

	secondStart, _, ok := m.messageBlockLineRange(2)
	require.True(t, ok)
	m.content.SetYOffset(secondStart)

	require.Equal(t, -1, m.stickyUserMessageIndex(m.content.YOffset()))
}

func TestStickyUserOverlayAtTopOfViewport(t *testing.T) {
	m := stickyScrollTestModel(t)
	anchor, ok := m.userMessageScrollAnchor(0)
	require.True(t, ok)
	m.content.SetYOffset(anchor + 1)

	view := stripANSI(m.contentBodyView())
	require.Contains(t, view, "▸")
	require.Contains(t, view, "user prompt line one")
	require.NotContains(t, view, "user prompt line two")
}

func TestStickyScrollShowsFullLastDetailBoxAtBottom(t *testing.T) {
	m := stickyScrollTestModel(t)
	m.height = 28
	m.messages = append(m.messages, message{
		kind:           uiconst.MessageDetail,
		detailLabel:    "Tool result",
		text:           strings.Repeat("detail body line\n", 6) + "detail footer marker",
		detailExpanded: true,
	})
	m.layout.ContentDirty = true
	m = m.syncLayout(true)
	require.True(t, m.content.AtBottom())

	view := stripANSI(m.contentBodyView())
	require.Contains(t, view, "detail footer marker")
	require.Contains(t, view, "click or ctrl+o to collapse")
}

func TestStickyDoesNotHideAIContentUnderOverlay(t *testing.T) {
	m := stickyScrollTestModel(t)
	_, userEnd, ok := m.messageBlockLineRange(0)
	require.True(t, ok)
	scrollTop := userEnd + 2
	m.content.SetYOffset(scrollTop)

	body := stripANSI(m.contentBodyView())
	lines := strings.Split(strings.TrimSuffix(body, "\n"), "\n")
	stickyH := m.stickyUserOverlayHeight(0)
	require.Greater(t, stickyH, 0)
	require.Greater(t, len(lines), stickyH)

	contentLines := strings.Split(m.content.GetContent(), "\n")
	expected := strings.TrimSpace(stripANSI(contentLines[scrollTop]))
	actual := strings.TrimSpace(lines[stickyH])
	require.Contains(t, actual, expected)
}

func TestStickyUserClickTogglesExpand(t *testing.T) {
	m := stickyScrollTestModel(t)
	_, userEnd, ok := m.messageBlockLineRange(0)
	require.True(t, ok)
	m.content.SetYOffset(userEnd + 3)
	m = m.syncLayout(false)

	updated, _ := m.Update(mouseClick(2, 0, tea.MouseLeft, 0))
	m = updated.(Model)
	require.True(t, m.messages[0].detailExpanded)
}

func TestUserMessageBoxesRenderLeftBorder(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	sticky := renderUserSticky(60, "alpha\nbeta", at)
	collapsed := renderUserCollapsible(60, "alpha\nbeta", false, at)
	require.Contains(t, sticky, "▎")
	require.Contains(t, collapsed, "▎")
}

func TestUserLeftBarHeightMatchesDetailBox(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	collapsed := renderUserCollapsible(60, "alpha\nbeta\ngamma", false, at)
	height := lipgloss.Height(collapsed)

	plain := stripANSI(collapsed)
	barLines := 0
	for _, line := range strings.Split(plain, "\n") {
		trimmed := strings.TrimLeft(line, " ")
		if trimmed == "" {
			continue
		}
		if strings.HasPrefix(trimmed, "▎") {
			barLines++
		}
	}
	require.Equal(t, height, barLines, "each box row should include the left accent bar")
}

func TestRenderUserStickyIsCompactCollapsed(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	sticky := stripANSI(renderUserSticky(60, "alpha\nbeta", at))
	full := stripANSI(renderUserCollapsible(60, "alpha\nbeta", false, at))

	require.Contains(t, sticky, "▸")
	require.Contains(t, sticky, "alpha")
	require.NotContains(t, sticky, "beta")
	require.Contains(t, sticky, "09:30:00")
	require.NotContains(t, sticky, "click or ctrl+o")
	require.Contains(t, full, "click or ctrl+o to expand")
}

func TestRenderUserStickyTitleAndTimestampUseDistinctStyles(t *testing.T) {
	now := time.Now()
	at := time.Date(now.Year(), now.Month(), now.Day(), 9, 30, 0, 0, time.Local)
	raw := renderUserSticky(60, "hello\nworld", at)

	require.Contains(t, raw, uiconst.StickyUserTitleStyle().Render("▸ hello"))
	require.Contains(t, raw, uiconst.StickyUserTimestampStyle().Render("09:30:00"))
	require.NotContains(t, raw, uiconst.StickyUserTimestampStyle().Render("hello"))
}

func TestRenderUserStickyTimestampOnTitleLine(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	sticky := stripANSI(renderUserSticky(60, "alpha\nbeta", at))

	var titleLine string
	for _, line := range strings.Split(sticky, "\n") {
		if strings.Contains(line, "▸") {
			titleLine = line
			break
		}
	}
	require.Contains(t, titleLine, "alpha")
	require.Contains(t, titleLine, "09:30:00")
}

func TestRenderUserStickyTruncatesTitleForTimestamp(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	long := strings.Repeat("x", 80) + "\nsecond"
	sticky := stripANSI(renderUserSticky(36, long, at))
	require.Contains(t, sticky, "09:30:00")
	require.NotContains(t, sticky, strings.Repeat("x", 20))
}

func TestStickyUserFooterRowPadsGapWithStickyBackground(t *testing.T) {
	left := uiconst.StickyUserTitleStyle().Render("▸ " + strings.Repeat("x", 8) + "...")
	right := uiconst.StickyUserTimestampStyle().Render("14:25:49")
	contentW := 40

	row := stickyUserFooterRow(contentW, left, right)
	require.Equal(t, contentW, lipgloss.Width(row))

	leftW := contentW - lipgloss.Width(right)
	gap := leftW - lipgloss.Width(left)
	require.GreaterOrEqual(t, gap, align.ColumnGap)

	expectedPad := uiconst.StickyUserStyle().Render(strings.Repeat(" ", gap))
	require.Contains(t, row, expectedPad)
}

func TestRenderUserStickyTruncatedTitleHasGapBeforeTimestamp(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	long := strings.Repeat("x", 80) + "\nsecond"
	sticky := stripANSI(renderUserSticky(36, long, at))

	var titleLine string
	for _, line := range strings.Split(sticky, "\n") {
		if strings.Contains(line, "...") && strings.Contains(line, "09:30:00") {
			titleLine = line
			break
		}
	}
	require.NotEmpty(t, titleLine)

	ellipsis := strings.Index(titleLine, "...")
	timeIdx := strings.Index(titleLine, "09:30:00")
	require.Greater(t, timeIdx, ellipsis+3)
	require.GreaterOrEqual(t, timeIdx-(ellipsis+3), align.ColumnGap)
}

func TestRenderUserStickyIsShorterThanCollapsedFooter(t *testing.T) {
	at := time.Date(2026, 6, 15, 9, 30, 0, 0, time.Local)
	width := 60

	sticky := renderUserSticky(width, "alpha\nbeta", at)
	collapsed := renderUserCollapsible(width, "alpha\nbeta", false, at)
	require.Less(t, lipgloss.Height(sticky), lipgloss.Height(collapsed))
}
