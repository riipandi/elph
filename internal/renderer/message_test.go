package renderer

import (
	"fmt"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/riipandi/elph/internal/uiconst"
	"github.com/stretchr/testify/require"
)

func testModel() Model {
	m := withActiveTestModel(New())
	m.width = 80
	m.content.SetWidth(80)
	return m
}

func TestMessageKindsNoPipePrefix(t *testing.T) {
	m := testModel()
	kinds := []uiconst.MessageKind{
		uiconst.MessageUser,
		uiconst.MessageAI,
		uiconst.MessageSystem,
		uiconst.MessageTool,
		uiconst.MessageThinking,
	}
	for _, kind := range kinds {
		rendered := m.renderMessage(message{text: "sample text", kind: kind})
		require.NotContains(t, stripANSI(rendered), "| ",
			"kind %d should not use pipe prefix", kind)
	}
}

func TestMessageKindsNoChevronPrefix(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "Copied to clipboard", kind: uiconst.MessageSystem})
	require.False(t, strings.HasPrefix(strings.TrimSpace(stripANSI(rendered)), ">"),
		"system message should not use > prefix: %q", stripANSI(rendered))
}

func TestUserMessageStyled(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "hello from user", kind: uiconst.MessageUser})
	require.Contains(t, rendered, "hello from user")
}

func TestAIMessageRendersText(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "response from agent", kind: uiconst.MessageAI})
	require.Contains(t, rendered, "response from agent")
}

func TestResponseMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	messages := []message{
		promptSpacingMessage("thinking step", uiconst.MessageThinking),
		{text: "from agent", kind: uiconst.MessageAI},
		{text: "from user", kind: uiconst.MessageUser},
		{text: "reply", kind: uiconst.MessageAI},
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

func TestAIMessageNoHorizontalPadding(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{text: "response from agent", kind: uiconst.MessageAI}))
	require.False(t, strings.HasPrefix(rendered, " "), "AI message should not have horizontal padding: %q", rendered)
	require.Equal(t, m.messageAreaWidth(), lipgloss.Width(m.renderMessage(message{text: "response from agent", kind: uiconst.MessageAI})))
}

func TestAIMessageHasBottomPaddingOnly(t *testing.T) {
	m := testModel()
	single := m.renderMessage(message{text: "only line", kind: uiconst.MessageAI})
	twoParas := m.renderMessage(message{text: "First paragraph ends.\n\nSecond paragraph starts.", kind: uiconst.MessageAI})
	multiLine := m.renderMessage(message{
		text: strings.Repeat("word ", 20) + "\n\n" + strings.Repeat("more ", 20),
		kind: uiconst.MessageAI,
	})
	require.Greater(t, lipgloss.Height(twoParas), lipgloss.Height(single),
		"paragraph gaps should add visible height")
	require.Greater(t, lipgloss.Height(multiLine), lipgloss.Height(single),
		"multi-paragraph replies should be taller than a single line")
}

func TestThinkingMessageUsesCollapsibleBox(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{
		text:        "reasoning",
		kind:        uiconst.MessageThinking,
		detailLabel: "Thinking",
	})
	require.Contains(t, stripANSI(rendered), "Thought")
	require.Contains(t, stripANSI(rendered), "ctrl+o to expand")
}

func TestBoxedMessageSingleLineHeight(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "hello", kind: uiconst.MessageUser})
	require.Equal(t, 3, lipgloss.Height(rendered), "single-line user block includes vertical padding")
}

func TestUserMessageVerticalSpacing(t *testing.T) {
	m := testModel()
	messages := []message{
		{text: "from agent", kind: uiconst.MessageAI},
		{text: "from user", kind: uiconst.MessageUser},
		{text: "reply", kind: uiconst.MessageAI},
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
		{text: "from agent", kind: uiconst.MessageAI},
		{text: "Copied to clipboard", kind: uiconst.MessageSystem},
	}
	content := normalizeSpacingLines(stripANSI(m.messagesView()))
	require.Contains(t, content, "from agent\n\n\n"+aiCopyHintText+"\n\nCopied to clipboard")
}

func TestUserMessageMultiline(t *testing.T) {
	m := testModel()
	collapsed := m.renderMessage(message{text: "line one\nline two", kind: uiconst.MessageUser})
	plain := stripANSI(collapsed)
	require.Contains(t, plain, "line one")
	require.NotContains(t, plain, "line two")
	require.GreaterOrEqual(t, lipgloss.Height(collapsed), 3)

	expanded := m.renderMessage(message{
		text:           "line one\nline two",
		kind:           uiconst.MessageUser,
		detailExpanded: true,
	})
	require.GreaterOrEqual(t, lipgloss.Height(expanded), 4,
		"expanded multiline user message should include vertical padding")
	plain = stripANSI(expanded)
	require.Contains(t, plain, "line one")
	require.Contains(t, plain, "line two")
}

func TestUserMessageWidthMatchesChrome(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "hello", kind: uiconst.MessageUser}}
	assertChromeWidthsMatch(t, m)
}

func TestUserMessageWidthMatchesChromeWithScrollbar(t *testing.T) {
	m := testModel()
	m.height = 12
	m.ready = true
	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: uiconst.MessageUser,
		})
	}
	m = m.syncLayout(false)
	assertChromeWidthsMatch(t, m)
}

func assertChromeWidthsMatch(t *testing.T, m Model) {
	t.Helper()
	userW := lipgloss.Width(m.renderMessage(m.messages[len(m.messages)-1]))
	msgW := m.messageAreaWidth()
	inputW := lipgloss.Width(m.inputView())
	require.Equal(t, msgW, userW, "user message width vs messageAreaWidth")
	require.Equal(t, m.chromeOuterWidth(), inputW, "input width vs chromeOuterWidth")
}

func TestMessageWidthUsesContentAreaWidth(t *testing.T) {
	m := testModel()
	m.height = 12
	m.ready = true
	for i := range 30 {
		m.messages = append(m.messages, message{
			text: fmt.Sprintf("message %d", i),
			kind: uiconst.MessageUser,
		})
	}
	m = m.syncLayout(false)

	areaW := m.contentAreaWidth()
	msgW := m.messageAreaWidth()
	require.Equal(t, m.width-scrollBarWidth, areaW)
	require.Equal(t, areaW-messageScrollInset, msgW)
	for _, kind := range []uiconst.MessageKind{
		uiconst.MessageUser,
		uiconst.MessageAI,
		uiconst.MessageSystem,
		uiconst.MessageTool,
		// MessageThinking excluded: compact format doesn't pad to full width
	} {
		renderedW := lipgloss.Width(m.renderMessage(message{text: "hello", kind: kind}))
		require.Equal(t, msgW, renderedW, "kind %d width", kind)
	}
}

func TestUserMessageHorizontalPadding(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{text: "hello", kind: uiconst.MessageUser}))
	require.True(t, strings.HasPrefix(rendered, "▎"), "user message should render a left accent bar: %q", rendered)
	require.Contains(t, rendered, "hello")
}

func TestToolMessageBlockPadding(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{text: "$ ls\nfile.txt", kind: uiconst.MessageTool})
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
	require.NotEqual(t, uiconst.DimText, uiconst.UserMsgBg,
		"user message background should differ from dim text")
	_ = lipgloss.NewStyle().Background(uiconst.UserMsgBg).Render("x")
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

// stripANSI is a test helper; lipgloss output includes CSI, OSC 8, and SGR sequences.
func stripANSI(s string) string {
	return ansi.Strip(s)
}
