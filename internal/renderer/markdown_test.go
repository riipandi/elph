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

func TestAIMessageRendersFencedCodeBlock(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "```go\nfmt.Println(\"hi\")\n```",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "fmt.Println")
	require.NotContains(t, rendered, "```")
}

func TestStripMarkdownSyntaxPreservesFences(t *testing.T) {
	got := stripMarkdownSyntax("```go\nfmt.Println()\n```")
	require.Contains(t, got, "```go")
	require.Contains(t, got, "```")
	require.Contains(t, got, "fmt.Println()")
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

func TestMarkdownSchedulesAsyncRender(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "| A | B |\n|---|---|\n| 1 | 2 |", kind: constants.MessageAI}}

	updated, cmd := m.scheduleMarkdownRender(0)
	require.NotNil(t, cmd)
	require.True(t, updated.messages[0].markdownPending)

	preview := stripANSI(updated.renderMessageAt(0))
	require.Contains(t, preview, "A")
	require.Contains(t, preview, "B")
}

func TestMarkdownRenderMsgUpdatesCache(t *testing.T) {
	m := testModel()
	source := "## hello"
	m.messages = []message{{text: source, kind: constants.MessageAI, markdownPending: true}}

	updated, cmd := m.handleMarkdownRenderMsg(markdownRenderMsg{
		index:  0,
		width:  m.messageAreaWidth(),
		source: source,
		output: renderAIMarkdown(m.messageAreaWidth(), source),
	})
	require.Nil(t, cmd)
	require.False(t, updated.messages[0].markdownPending)
	require.Contains(t, stripANSI(updated.renderMessageAt(0)), aiCopyHintText)
	require.True(t, updated.messages[0].renderCache.hit(m.messageAreaWidth(), false, len(source), false, updated.messages[0].detailStatus, updated.messages[0].at, collapsibleRenderOpts{}))
}

func TestAsyncMarkdownRenderIncludesCopyHint(t *testing.T) {
	m := testModel()
	source := "## Title\n\nBody paragraph."
	m.messages = []message{{text: source, kind: constants.MessageAI}}
	updated, cmd := m.scheduleMarkdownRender(0)
	require.NotNil(t, cmd)
	require.True(t, updated.messages[0].markdownPending)
	require.Contains(t, stripANSI(updated.renderMessageAt(0)), aiCopyHintText)

	msg := cmd()
	rendered, ok := msg.(markdownRenderMsg)
	require.True(t, ok)
	final, _ := updated.handleMarkdownRenderMsg(rendered)
	require.Contains(t, stripANSI(final.renderMessageAt(0)), aiCopyHintText)
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

func TestNormalizeAIProseSeparatorsStripsDashRules(t *testing.T) {
	text := "Para satu.\n\n--------\n\nPara dua."
	normalized := normalizeAIProseSeparators(text)
	require.NotContains(t, normalized, "--------")
	require.Contains(t, normalized, "\n\n")
}

func TestAIMessageRendersDashSeparatorAsHR(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "Para satu.\n\n--------\n\nPara dua.",
		kind: constants.MessageAI,
	}))
	require.Contains(t, raw, "--------")
	require.Contains(t, raw, "Para satu.")
	require.Contains(t, raw, "Para dua.")
}

func TestAIMessageRendersHorizontalRuleInMarkdown(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "**Bold** intro.\n\n---\n\nClosing paragraph.",
		kind: constants.MessageAI,
	}))
	require.Contains(t, raw, "--------")
	require.Contains(t, raw, "Bold")
	require.Contains(t, raw, "Closing paragraph.")
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

func TestMarkdownParagraphGapIsVisible(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "**Intro** line.\n\nSecond paragraph here.",
		kind: constants.MessageAI,
	}))
	lines := strings.Split(raw, "\n")
	introIdx, secondIdx := -1, -1
	for i, line := range lines {
		switch {
		case strings.Contains(line, "Intro"):
			introIdx = i
		case strings.Contains(line, "Second paragraph"):
			secondIdx = i
		}
	}
	require.NotEqual(t, -1, introIdx)
	require.NotEqual(t, -1, secondIdx)
	require.Greater(t, secondIdx-introIdx, 1, "markdown paragraphs should be separated by a blank line")
}

func TestSplitAIBlockParagraphsDetectsMarkdownSpacers(t *testing.T) {
	rendered := renderAIMarkdown(testModel().messageAreaWidth(), "First paragraph.\n\nSecond paragraph.")
	chunks := splitAIBlockParagraphs(rendered)
	require.Len(t, chunks, 2)
	require.Contains(t, stripANSI(chunks[0]), "First paragraph.")
	require.Contains(t, stripANSI(chunks[1]), "Second paragraph.")
}

func TestFormatAIProseSplitsShortIndonesianParagraphs(t *testing.T) {
	text := "Paragraf pertama selesai.\nParagraf kedua dimulai."
	formatted := formatAIProse(text, 80)
	require.Contains(t, formatted, "\n\n")
	require.Contains(t, formatted, "Paragraf pertama selesai.")
	require.Contains(t, formatted, "Paragraf kedua dimulai.")
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
	require.True(t, looksLikeMarkdown("This is *italic* text."))
	require.True(t, looksLikeMarkdown("This is _italic_ text."))
}

func TestSingleAsteriskItalicIsStyled(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{text: "This is *important* text.", kind: constants.MessageAI})
	require.Contains(t, stripANSI(raw), "important")
	require.NotContains(t, stripANSI(raw), "*important*")
	require.True(t, strings.Contains(raw, "[3m") || strings.Contains(raw, ";3m") ||
		strings.Contains(raw, "[1m") || strings.Contains(raw, ";1m") || strings.Contains(raw, ";1;3m"),
		"italic/bold ANSI should be present")
}

func TestMarkdownPendingShowsSourcePreview(t *testing.T) {
	m := testModel()
	m.messages = []message{{text: "**hello**", kind: constants.MessageAI, markdownPending: true}}
	raw := stripANSI(m.renderMessageAt(0))
	require.Contains(t, raw, "**hello**")
}

func TestNonAIMessagesSkipMarkdown(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "**literal**",
		kind: constants.MessageUser,
	}))
	require.Contains(t, rendered, "**literal**")
}

func TestMarkdownPendingShowsSourceLinkPreview(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:            "GitHub: [github.com/riipandi/elph](https://github.com/riipandi/elph)",
		kind:            constants.MessageAI,
		markdownPending: true,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "github.com/riipandi/elph")
	require.Contains(t, rendered, "](")
}

func TestAIMessageStripsDuplicateLinkInMarkdown(t *testing.T) {
	m := testModel()
	m.messages = []message{{
		text:            "visit [https://example.com](https://example.com) now",
		kind:            constants.MessageAI,
		markdownPending: false,
	}}

	rendered := stripANSI(m.renderMessageAt(0))
	count := strings.Count(rendered, "https://example.com")
	require.Equal(t, 1, count, "URL should not be duplicated in markdown output")
}

func TestAIMessageRendersStrikeAndBoldItalic(t *testing.T) {
	m := testModel()
	rendered := stripANSI(m.renderMessage(message{
		text: "~~old~~ and ***emphasis*** and `fmt.Println`",
		kind: constants.MessageAI,
	}))
	require.Contains(t, rendered, "old")
	require.Contains(t, rendered, "emphasis")
	require.Contains(t, rendered, "fmt.Println")
	require.NotContains(t, rendered, "~~")
	require.NotContains(t, rendered, "***")
}

func TestAIMessageRendersBlockquote(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "> Line one\n> Line two\n> Line three",
		kind: constants.MessageAI,
	}))
	require.Contains(t, raw, "Line one")
	require.Contains(t, raw, "Line two")
	require.Contains(t, raw, "Line three")
	require.NotContains(t, raw, "Line one Line two")
	require.NotContains(t, raw, "Line two Line three")
}

func TestStreamingBlockquoteRendersStyled(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.agent.ResponseMsgID = 0
	m.messages = []message{{text: "> Line one\n> Line two", kind: constants.MessageAI}}

	rendered := stripANSI(m.renderMessageAt(0))
	require.Contains(t, rendered, "Line one")
	require.Contains(t, rendered, "Line two")
}

func TestAIMessageMarkdownLinkNoOSCLeak(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{
		text: "Visit [GitHub](https://github.com/riipandi/elph) for more.",
		kind: constants.MessageAI,
	})
	plain := stripANSI(raw)
	require.Contains(t, plain, "GitHub")
	require.NotContains(t, plain, "https://github.com/riipandi/elph")
	require.NotContains(t, plain, "/riipandi/elphGitHub")
}

func TestAIMessageRendersTable(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "| Col A | Col B |\n|-------|-------|\n| one   | two   |",
		kind: constants.MessageAI,
	}))
	require.Contains(t, raw, "Col A")
	require.Contains(t, raw, "Col B")
	require.Contains(t, raw, "│")
	require.NotContains(t, raw, "|---|")
}

func TestAIMessageRendersNestedBlockquote(t *testing.T) {
	m := testModel()
	raw := stripANSI(m.renderMessage(message{
		text: "> Outer quote\n> > Nested quote\n> Still outer",
		kind: constants.MessageAI,
	}))
	require.Contains(t, raw, "Outer quote")
	require.Contains(t, raw, "Nested quote")
	require.Contains(t, raw, "Still outer")
	require.NotContains(t, raw, "> Nested")
	require.NotContains(t, raw, "Nested quote Still outer")
	require.NotContains(t, raw, "Outer quote Nested quote")
}

func TestLooksLikeMarkdownDetectsTable(t *testing.T) {
	require.True(t, looksLikeMarkdown("| A | B |"))
	require.True(t, hasMarkdownBlockStructure("| A | B |\n|---|---|"))
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
