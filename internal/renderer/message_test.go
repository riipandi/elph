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

func TestUserMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	m.messages = []message{
		{text: "from agent", kind: constants.MessageAI},
		{text: "from user", kind: constants.MessageUser},
		{text: "reply", kind: constants.MessageAI},
	}
	content := stripANSI(m.contentView())
	agentEnd := strings.Index(content, "from agent")
	userStart := strings.Index(content, "from user")
	replyStart := strings.Index(content, "reply")
	require.GreaterOrEqual(t, agentEnd, 0)
	require.GreaterOrEqual(t, userStart, 0)
	require.GreaterOrEqual(t, replyStart, 0)
	require.Contains(t, content[agentEnd:userStart], "\n\n", "blank line between AI and user message")
	require.Contains(t, content[userStart:replyStart], "\n\n", "blank line between user and AI message")
}

func TestSystemMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	m.messages = []message{
		{text: "from agent", kind: constants.MessageAI},
		{text: "Copied to clipboard", kind: constants.MessageSystem},
	}
	content := stripANSI(m.contentView())
	agentEnd := strings.Index(content, "from agent")
	systemStart := strings.Index(content, "Copied to clipboard")
	require.GreaterOrEqual(t, agentEnd, 0)
	require.GreaterOrEqual(t, systemStart, 0)
	require.Contains(t, content[agentEnd:systemStart], "\n\n",
		"blank line above system message")
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

// stripANSI is a minimal helper for tests; lipgloss output includes sequences.
func stripANSI(s string) string {
	var b strings.Builder
	inEsc := false
	for _, r := range s {
		if r == '\x1b' {
			inEsc = true
			continue
		}
		if inEsc {
			if r == 'm' {
				inEsc = false
			}
			continue
		}
		b.WriteRune(r)
	}
	return b.String()
}
