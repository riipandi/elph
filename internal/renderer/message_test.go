package renderer

import (
	"fmt"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func testModel() Model {
	m := New()
	m.width = 80
	m.content.SetWidth(80)
	return m
}

func TestMessageKindsNoPipePrefix(t *testing.T) {
	m := testModel()
	kinds := []constants.MessageKind{
		constants.MessageUser,
		constants.MessageAI,
		constants.MessageSystem,
		constants.MessageTool,
		constants.MessageThinking,
	}
	for _, kind := range kinds {
		rendered := m.renderMessage(message{text: "sample text", kind: kind})
		require.NotContains(t, stripANSI(rendered), "| ",
			"kind %d should not use pipe prefix", kind)
	}
}

func TestMessageKindsNoChevronPrefix(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "Copied to clipboard", kind: constants.MessageSystem})
	require.False(t, strings.HasPrefix(strings.TrimSpace(stripANSI(rendered)), ">"),
		"system message should not use > prefix: %q", stripANSI(rendered))
}

func TestUserMessageStyled(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "hello from user", kind: constants.MessageUser})
	require.Contains(t, rendered, "hello from user")
}

func TestAIMessageRendersText(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "response from agent", kind: constants.MessageAI})
	require.Contains(t, rendered, "response from agent")
}

func TestResponseMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	messages := []message{
		promptSpacingMessage("thinking step", constants.MessageThinking),
		{text: "from agent", kind: constants.MessageAI},
		{text: "from user", kind: constants.MessageUser},
		{text: "reply", kind: constants.MessageAI},
	}
	m.messages = messages
	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	for i := 1; i < len(messages); i++ {
		prev, curr := messages[i-1], messages[i]
		want := expectedBlankLinesBetween(prev.kind, curr.kind)
		blanks := blankLinesBetweenMarkers(content, spacingMarker(prev.text, prev.kind), spacingMarker(curr.text, curr.kind))
		require.Equal(t, want, blanks, "%s -> %s", prev.text, curr.text)
	}
}

func TestAIMessageHasBottomPaddingOnly(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "line one\nline two", kind: constants.MessageAI})
	require.Equal(t, 3, lipgloss.Height(rendered), "AI blocks add one line below the last row")
}

func TestThinkingMessageUsesCollapsibleBox(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{
		text:        "reasoning",
		kind:        constants.MessageThinking,
		detailLabel: "Thinking",
	})
	require.GreaterOrEqual(t, lipgloss.Height(rendered), 3)
	require.Contains(t, stripANSI(rendered), "ctrl+o to expand")
}

func TestBoxedMessageSingleLineHeight(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "hello", kind: constants.MessageUser})
	require.Equal(t, 3, lipgloss.Height(rendered), "user block includes vertical padding")
}

func TestUserMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	messages := []message{
		{text: "from agent", kind: constants.MessageAI},
		{text: "from user", kind: constants.MessageUser},
		{text: "reply", kind: constants.MessageAI},
	}
	m.messages = messages
	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	for i := 1; i < len(messages); i++ {
		prev, curr := messages[i-1], messages[i]
		want := expectedBlankLinesBetween(prev.kind, curr.kind)
		blanks := blankLinesBetweenMarkers(content, prev.text, curr.text)
		require.Equal(t, want, blanks, "%s -> %s", prev.text, curr.text)
	}
}

func TestSystemMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	m.messages = []message{
		{text: "from agent", kind: constants.MessageAI},
		{text: "Copied to clipboard", kind: constants.MessageSystem},
	}
	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	require.Contains(t, content, "from agent\n\n\nCopied to clipboard")
}

func TestUserMessageMultiline(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "line one\nline two", kind: constants.MessageUser})
	require.GreaterOrEqual(t, lipgloss.Height(rendered), 4,
		"multiline user message should include vertical padding")
	plain := stripANSI(rendered)
	require.Contains(t, plain, "line one")
	require.Contains(t, plain, "line two")
}

func TestUserMessageWidthMatchesChrome(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "hello", kind: constants.MessageUser}}
	assertChromeWidthsMatch(t, m)
}

func TestUserMessageWidthMatchesChromeWithScrollbar(t *testing.T) {
	m := testModel()
	m.height = 12
	m.ready = true
	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: constants.MessageUser,
		})
	}
	m = m.syncLayout(false)
	assertChromeWidthsMatch(t, m)
}

func assertChromeWidthsMatch(t *testing.T, m Model) {
	t.Helper()
	userW := lipgloss.Width(m.renderMessage(m.messages[len(m.messages)-1]))
	bannerW := lipgloss.Width(m.bannerView())
	inputW := lipgloss.Width(m.inputView())
	msgW := m.messageAreaWidth()
	require.Equal(t, msgW, userW, "user message width vs messageAreaWidth")
	require.Equal(t, m.chromeOuterWidth(), bannerW, "banner width vs chromeOuterWidth")
	require.Equal(t, bannerW, inputW, "input width vs banner width")
}

func TestMessageWidthUsesContentAreaWidth(t *testing.T) {
	m := testModel()
	m.height = 12
	m.ready = true
	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: constants.MessageUser,
		})
	}
	m = m.syncLayout(false)

	areaW := m.contentAreaWidth()
	msgW := m.messageAreaWidth()
	require.Equal(t, m.width-scrollBarWidth, areaW)
	require.Equal(t, areaW-messageScrollInset, msgW)
	for _, kind := range []constants.MessageKind{
		constants.MessageUser,
		constants.MessageAI,
		constants.MessageSystem,
		constants.MessageTool,
		constants.MessageThinking,
	} {
		renderedW := lipgloss.Width(m.renderMessage(message{text: "hello", kind: kind}))
		require.Equal(t, msgW, renderedW, "kind %d width", kind)
	}
}

func TestUserMessageHorizontalPadding(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{text: "hello", kind: constants.MessageUser}))
	require.True(t, strings.HasPrefix(rendered, "  "), "user message should have horizontal padding: %q", rendered)
}

func TestToolMessageBlockPadding(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "$ ls\nfile.txt", kind: constants.MessageTool})
	require.GreaterOrEqual(t, lipgloss.Height(rendered), 4,
		"multiline tool message should include vertical padding")
	plain := stripANSI(rendered)
	require.Contains(t, plain, "$ ls")
	require.Contains(t, plain, "file.txt")
	require.True(t, strings.HasPrefix(plain, "  "), "tool message should have horizontal padding: %q", plain)

	msgW := m.messageAreaWidth()
	require.Equal(t, msgW, lipgloss.Width(rendered))
}

func TestUserMsgBgConstant(t *testing.T) {
	require.NotEqual(t, constants.DimText, constants.UserMsgBg,
		"user message background should differ from dim text")
	_ = lipgloss.NewStyle().Background(constants.UserMsgBg).Render("x")
}

// normalizeSpacingLines collapses whitespace-only lines so spacing assertions
// are not thrown off by lipgloss width padding.
func normalizeSpacingLines(s string) string {
	lines := strings.Split(s, "\n")
	out := make([]string, 0, len(lines))
	for _, line := range lines {
		if strings.TrimSpace(line) == "" {
			out = append(out, "")
			continue
		}
		out = append(out, strings.TrimSpace(line))
	}
	return strings.TrimSpace(strings.Join(out, "\n"))
}

// stripANSI is a minimal helper for tests; lipgloss output includes sequences.
func stripANSI(s string) string {
	var b strings.Builder
	inEsc := false
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c == '\x1b' {
			inEsc = true
			continue
		}
		if inEsc {
			// CSI (ESC[) ends with 0x40-0x7E (e.g. 'm' for SGR).
			// OSC (ESC]) ends with ST (ESC\) or BEL (0x07).
			if c == 'm' || c == '\\' || c == 0x07 {
				inEsc = false
			}
			continue
		}
		b.WriteByte(c)
	}
	return b.String()
}
