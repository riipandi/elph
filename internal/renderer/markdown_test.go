package renderer

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestAIMessageRendersMarkdownBold(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "**important** update",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "important")
	require.NotContains(t, rendered, "**")
}

func TestAIMessageRendersMarkdownHeading(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "## Summary\n\nDetails here.",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "Summary")
}

func TestAIMessageRendersMarkdownCodeBlock(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "Use `fmt.Println` here",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "fmt.Println")
	require.NotContains(t, rendered, "`")
}

func TestStreamingUsesPlainPathWithoutGlamour(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.agent.ResponseMsgID = 0
	m.messages = []message{{text: "**partial**", kind: constants.MessageAI}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "**partial**")
}

func TestPlainStreamingStaysPlainUntilComplete(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.agent.ResponseMsgID = 0
	m.messages = []message{{text: "Hello there", kind: constants.MessageAI}}

	plain := stripANSI(m.renderMessageAt(0))
	require.Contains(t, plain, "Hello there")

	m.agent.Busy = false
	m.agent.ResponseMsgID = -1
	m.messages[0].text = "Hello there\n\n**done**"
	m.messages[0].renderCache = messageRenderCache{}

	formatted := stripANSI(m.renderMessage(message{
		text: m.messages[0].text,
		kind: constants.MessageAI,
	}))
	require.Contains(t, formatted, "done")
	require.NotContains(t, formatted, "**")
}

func TestMessageRenderCacheAvoidsRepeatWork(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "plain ai reply", kind: constants.MessageAI}}

	first := m.renderMessageAt(0)
	second := m.renderMessageAt(0)
	require.Equal(t, first, second)
	require.True(t, m.messages[0].renderCache.hit(m.messageAreaWidth(), false, len(m.messages[0].text), false, m.messages[0].detailStatus, m.messages[0].at, collapsibleRenderOpts{}))
}

func TestMarkdownSchedulesAsyncGlamour(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "**hello**", kind: constants.MessageAI}}

	updated, cmd := m.scheduleGlamourRender(0)
	require.NotNil(t, cmd)
	require.True(t, updated.messages[0].glamourPending)

	preview := stripANSI(updated.renderMessageAt(0))
	require.Contains(t, preview, "hello")
	require.NotContains(t, preview, "**")
}

func TestGlamourRenderMsgUpdatesCache(t *testing.T) {
	m := testModel()
	source := "**hello**"
	m.messages = []message{{text: source, kind: constants.MessageAI, glamourPending: true}}

	updated, cmd := m.handleGlamourRenderMsg(glamourRenderMsg{
		index:  0,
		width:  m.messageAreaWidth(),
		source: source,
		output: renderAIMessageGlamour(m.messageAreaWidth(), source),
	})
	require.Nil(t, cmd)
	require.False(t, updated.messages[0].glamourPending)
	require.True(t, updated.messages[0].renderCache.hit(m.messageAreaWidth(), false, len(source), false, updated.messages[0].detailStatus, updated.messages[0].at, collapsibleRenderOpts{}))
}

func TestAIMarkdownListHasBottomPadding(t *testing.T) {
	m := testModel()
	plain := strings.TrimSpace(`Intro line

- item one
- item two

Closing line.`)
	formatted := m.renderMessage(message{text: plain, kind: constants.MessageAI})
	plainRendered := m.renderMessage(message{text: "single", kind: constants.MessageAI})
	require.Greater(t, lipgloss.Height(formatted), lipgloss.Height(plainRendered),
		"markdown list blocks should be taller than a single-line reply due to bottom padding")
}

func TestAIMarkdownPreservesBlockWidth(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{
		text: "# Title\n\nA longer markdown paragraph that should wrap inside the message block.",
		kind: constants.MessageAI,
	})
	require.LessOrEqual(t, lipgloss.Width(rendered), m.messageAreaWidth())
}

func TestFormatAIProseJoinsSoftWrappedLines(t *testing.T) {
	text := "Rajin\nmenghitung uang rakyat, khususnya."
	formatted := formatAIProse(text, 40)
	require.NotContains(t, formatted, "Rajin\nmenghitung")
	require.Contains(t, formatted, "Rajin menghitung")
	require.NotContains(t, formatted, "-\n")
}

func TestFormatAIProsePreservesParagraphBreaks(t *testing.T) {
	text := "Paragraph one ends here.\nParagraph two starts now.\n\nParagraph three."
	formatted := formatAIProse(text, 80)
	require.Contains(t, formatted, "\n\n")
	require.Contains(t, formatted, "Paragraph one ends here.")
	require.Contains(t, formatted, "Paragraph two starts now.")
	require.Contains(t, formatted, "Paragraph three.")
	require.Contains(t, formatted, "here.\n\nParagraph two")
}

func TestFormatAIProseJoinsHyphenatedWrap(t *testing.T) {
	text := "melik-\nsipu di pantai."
	formatted := formatAIProse(text, 40)
	require.Contains(t, formatted, "meliksipu")
	require.NotContains(t, formatted, "melik-")
}

func TestFormatAIProseSplitsShortParagraphLines(t *testing.T) {
	text := "First paragraph ends.\nSecond paragraph starts."
	formatted := formatAIProse(text, 80)
	require.Contains(t, formatted, "\n\n")
	require.Contains(t, formatted, "First paragraph ends.")
	require.Contains(t, formatted, "Second paragraph starts.")
}

func TestFormatAIProseDoesNotSplitSoftWrappedLine(t *testing.T) {
	line1 := strings.Repeat("word ", 15) + "ends."
	text := line1 + "\nNext chunk continues here."
	paras := splitAIProseParagraphs(text, 80)
	require.Len(t, paras, 1)
	require.Contains(t, paras[0], "ends. Next")
}

func TestAIProseParagraphGapIsVisible(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "First paragraph ends.\nSecond paragraph starts.",
		kind: constants.MessageAI,
	}))
	lines := strings.Split(raw, "\n")
	firstIdx, secondIdx := -1, -1
	for i, line := range lines {
		switch {
		case strings.Contains(line, "First paragraph"):
			firstIdx = i
		case strings.Contains(line, "Second paragraph"):
			secondIdx = i
		}
	}
	require.NotEqual(t, -1, firstIdx)
	require.NotEqual(t, -1, secondIdx)
	require.Greater(t, secondIdx-firstIdx, 1, "paragraphs should be separated by a blank line")
}

func TestPlainAIMessageReflowsWithoutHyphenation(t *testing.T) {
	m := testModel()
	long := strings.Repeat("word ", 30)
	rendered := stripANSI(m.renderMessage(message{
		text: long,
		kind: constants.MessageAI,
	}))
	require.NotContains(t, rendered, "-\n")
}

func TestPlainAIMessageSkipsMarkdownRenderer(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "[[answer]]",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "[[answer]]")
}

func TestLooksLikeMarkdown(t *testing.T) {
	require.False(t, looksLikeMarkdown("[[answer]]"))
	require.False(t, looksLikeMarkdown("plain response"))
	require.True(t, looksLikeMarkdown("## Title"))
	require.True(t, looksLikeMarkdown("**bold**"))
	require.True(t, looksLikeMarkdown("- item"))
}

func TestNonAIMessagesSkipMarkdown(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "**literal**",
		kind: constants.MessageUser,
	}))
	require.Contains(t, rendered, "**literal**")
}

func TestAIMessageStripsMarkdownLinksInPlain(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:           "GitHub: [github.com/riipandi/elph](https://github.com/riipandi/elph)",
		kind:           constants.MessageAI,
		glamourPending: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "GitHub:")
	require.Contains(t, rendered, "github.com/riipandi/elph")
	// The URL is embedded as an OSC 8 hyperlink (not visible text).
	// Verify the raw output has the hyperlink before ANSI stripping.
	// The raw markdown syntax [ and ] should not appear
	require.NotContains(t, rendered, "[")
	raw := m.renderMessageAt(0)
	require.Contains(t, raw, "\x1b]8;;https://github.com/riipandi/elph\x1b\\")
	require.Contains(t, raw, "\x1b]8;;\x1b\\")
	require.NotContains(t, rendered, "](")
}

func TestAIMessageStripsDuplicateLinkInPlain(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:           "visit [https://example.com](https://example.com) now",
		kind:           constants.MessageAI,
		glamourPending: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	count := strings.Count(rendered, "https://example.com")
	// URL should appear exactly once, not "url (url)"
	require.Equal(t, 1, count, "URL should not be duplicated")
}

func TestAIMessageStripsDuplicateLinkInGlamour(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:           "visit [https://example.com](https://example.com) now",
		kind:           constants.MessageAI,
		glamourPending: false,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	count := strings.Count(rendered, "https://example.com")
	// URL should appear exactly once even in glamour path
	require.Equal(t, 1, count, "URL should not be duplicated in glamour")
}

func TestAIMessageStripsMarkdownSyntaxPreGlamour(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "**bold** and `code` and [link](https://example.com)", kind: constants.MessageAI, glamourPending: true}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "bold")
	require.Contains(t, rendered, "code")
	require.Contains(t, rendered, "link")
	require.NotContains(t, rendered, "**")
	require.NotContains(t, rendered, "`")
	require.NotContains(t, rendered, "](")
}

func TestStripMarkdownSyntax(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"bold", "**bold**", "bold"},
		{"bold alt", "__bold__", "bold"},
		{"code", "`code`", "code"},
		{"link", "[text](url)", "text"},
		{"italic", "*italic*", "italic"},
		{"italic alt", "_italic_", "italic"},
		{"mixed", "**bold** and `code`", "bold and code"},
		{"link in sentence", "visit [GitHub](https://github.com) now", "visit GitHub now"},
		{"link text eq url", "[https://example.com](https://example.com)", "https://example.com"},
		{"heading in sentence", "see # section below", "see # section below"},
		{"heading with inline", "# **bold** title", "bold title"},
		{"multiple headings", "# First\n\n## Second\n\n### Third", "First\n\nSecond\n\nThird"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := stripANSI(stripMarkdownSyntax(tt.input))
			require.Equal(t, tt.want, got)
		})
	}
}
