package rendermd

import (
	"testing"

	"github.com/stretchr/testify/require"
)

// ─── Helper / Predicate tests ──────────────────────────────────────────────

func TestIsHorizontalRuleLine(t *testing.T) {
	require.True(t, isHorizontalRuleLine("---"))
	require.True(t, isHorizontalRuleLine("***"))
	require.True(t, isHorizontalRuleLine("___"))
	require.True(t, isHorizontalRuleLine("-----"))
	require.False(t, isHorizontalRuleLine(""))
	require.False(t, isHorizontalRuleLine("--"))
	require.False(t, isHorizontalRuleLine("-a-"))
	require.False(t, isHorizontalRuleLine("==="))
}

func TestIsProseSeparatorLine(t *testing.T) {
	require.True(t, IsProseSeparatorLine("---"))
	require.True(t, IsProseSeparatorLine("==="))
	require.True(t, IsProseSeparatorLine("***"))
	require.True(t, IsProseSeparatorLine("___"))
	require.True(t, IsProseSeparatorLine("-----"))
	require.False(t, IsProseSeparatorLine(""))
	require.False(t, IsProseSeparatorLine("--"))
	require.False(t, IsProseSeparatorLine("-a-"))
}

func TestNormalizeProseSeparators(t *testing.T) {
	in := "Hello\n---\nWorld"
	got := NormalizeProseSeparators(in)
	require.Contains(t, got, "Hello\n\nWorld")
	require.NotContains(t, got, "---")
}

func TestNormalizeProseSeparators_DoubleSeparator(t *testing.T) {
	in := "A\n---\n---\nB"
	got := NormalizeProseSeparators(in)
	// Two consecutive separators collapse into a single blank line
	require.Equal(t, "A\n\nB", got)
}

func TestNormalizeProseSeparators_EmptyInput(t *testing.T) {
	require.Equal(t, "", NormalizeProseSeparators(""))
}

func TestLooksLikeTableLine(t *testing.T) {
	require.True(t, looksLikeTableLine("| a | b |"))
	require.True(t, looksLikeTableLine("a | b | c"))
	require.False(t, looksLikeTableLine("hello"))
	require.False(t, looksLikeTableLine(""))
}

func TestLooksLikeTableLine_SinglePipe(t *testing.T) {
	// A single pipe that doesn't start with | should be false
	require.False(t, looksLikeTableLine("a |"))
}

func TestHasMarkdownBlockStructure_CodeFence(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("some text\n```go\nfmt.Println()\n```"))
}

func TestHasMarkdownBlockStructure_Heading(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("# Title"))
	require.True(t, HasMarkdownBlockStructure("## Subtitle"))
}

func TestHasMarkdownBlockStructure_Blockquote(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("> quote"))
}

func TestHasMarkdownBlockStructure_OrderedList(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("1. item"))
}

func TestHasMarkdownBlockStructure_UnorderedList(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("- item"))
	require.True(t, HasMarkdownBlockStructure("* item"))
	require.True(t, HasMarkdownBlockStructure("+ item"))
}

func TestHasMarkdownBlockStructure_Table(t *testing.T) {
	require.True(t, HasMarkdownBlockStructure("a | b | c"))
}

func TestHasMarkdownBlockStructure_PlainText(t *testing.T) {
	require.False(t, HasMarkdownBlockStructure("just some plain text"))
}

func TestHasMarkdownBlockStructure_Empty(t *testing.T) {
	require.False(t, HasMarkdownBlockStructure(""))
}

func TestLooksLikeMarkdown_BlockStructure(t *testing.T) {
	require.True(t, LooksLikeMarkdown("# heading"))
}

func TestLooksLikeMarkdown_InlineBold(t *testing.T) {
	require.True(t, LooksLikeMarkdown("this is **bold** text"))
}

func TestLooksLikeMarkdown_InlineCode(t *testing.T) {
	require.True(t, LooksLikeMarkdown("use `code` here"))
}

func TestLooksLikeMarkdown_InlineLink(t *testing.T) {
	require.True(t, LooksLikeMarkdown("see [link](https://example.com)"))
}

func TestLooksLikeMarkdown_InlineImage(t *testing.T) {
	require.True(t, LooksLikeMarkdown("image ![alt](img.png)"))
}

func TestLooksLikeMarkdown_InlineItalic(t *testing.T) {
	require.True(t, LooksLikeMarkdown("some *italic* text"))
}

func TestLooksLikeMarkdown_PlainText(t *testing.T) {
	require.False(t, LooksLikeMarkdown("just plain text"))
}

func TestHasInlineAsteriskEmphasis(t *testing.T) {
	require.True(t, hasInlineAsteriskEmphasis("hello *world*"))
	require.False(t, hasInlineAsteriskEmphasis("hello world"))
	require.True(t, hasInlineAsteriskEmphasis("**bold**")) // "*bold*" found inside "**bold**"
	require.False(t, hasInlineAsteriskEmphasis("* "))
	require.False(t, hasInlineAsteriskEmphasis("*"))
}

func TestHasInlineUnderscoreEmphasis(t *testing.T) {
	require.True(t, hasInlineUnderscoreEmphasis("hello _world_"))
	require.False(t, hasInlineUnderscoreEmphasis("hello world"))
	require.True(t, hasInlineUnderscoreEmphasis("__bold__")) // "_bold_" found inside "__bold__"
	require.False(t, hasInlineUnderscoreEmphasis("_ "))
	require.False(t, hasInlineUnderscoreEmphasis("_"))
}

func TestEndsAISentence(t *testing.T) {
	require.True(t, endsAISentence("Hello."))
	require.True(t, endsAISentence("Hello!"))
	require.True(t, endsAISentence("Hello?"))
	require.False(t, endsAISentence("Hello,"))
	require.False(t, endsAISentence("Hello"))
	require.False(t, endsAISentence(""))
}

func TestStartsAIParagraphLine(t *testing.T) {
	require.True(t, startsAIParagraphLine("Hello world"))
	require.True(t, startsAIParagraphLine("1. item"))
	require.False(t, startsAIParagraphLine("hello world"))
	require.False(t, startsAIParagraphLine(""))
	require.False(t, startsAIParagraphLine("  "))
}

func TestCollapseExtraBlankLines(t *testing.T) {
	in := "a\n\n\n\n\nb"
	got := collapseExtraBlankLines(in)
	require.Equal(t, "a\n\nb", got)
}

func TestCollapseExtraBlankLines_TrimsEdge(t *testing.T) {
	got := collapseExtraBlankLines("  \nhello\n\n  ")
	require.Equal(t, "hello", got)
}

func TestCollapseDuplicateMarkdownLinks_NoChange(t *testing.T) {
	got := collapseDuplicateMarkdownLinks("normal text [text](url) more")
	require.Equal(t, "normal text [text](url) more", got)
}

func TestCollapseDuplicateMarkdownLinks_RemovesDuplicate(t *testing.T) {
	got := collapseDuplicateMarkdownLinks("[same](same)")
	require.Equal(t, "same", got)
}

func TestCollapseDuplicateMarkdownLinks_Multiple(t *testing.T) {
	got := collapseDuplicateMarkdownLinks("[same](same) and [diff](other)")
	require.Equal(t, "same and [diff](other)", got)
}

// ─── Sentence / Paragraph helpers ──────────────────────────────────────────

func TestJoinAIProseLines_Basic(t *testing.T) {
	got := joinAIProseLines("hello", "world")
	require.Equal(t, "hello world", got)
}

func TestJoinAIProseLines_EmptyPrev(t *testing.T) {
	require.Equal(t, "next", joinAIProseLines("", "next"))
}

func TestJoinAIProseLines_EmptyNext(t *testing.T) {
	require.Equal(t, "prev", joinAIProseLines("prev", ""))
}

func TestJoinAIProseLines_HyphenContinuation(t *testing.T) {
	got := joinAIProseLines("contin-", "uation")
	require.Equal(t, "continuation", got)
}

func TestJoinAIProseLines_HyphenNonLower(t *testing.T) {
	got := joinAIProseLines("pre-", "Processing")
	require.Equal(t, "pre- Processing", got)
}

func TestJoinAIProseLines_SentenceEndChars(t *testing.T) {
	require.Equal(t, "hello.world", joinAIProseLines("hello", ".world"))
	require.Equal(t, "hello,world", joinAIProseLines("hello", ",world"))
}

func TestShouldAIProseParagraphBreak(t *testing.T) {
	// Lowercase next should prevent break
	require.False(t, shouldAIProseParagraphBreak("Hello.", "world", 80))
	// Sentence end + uppercase next = break
	require.True(t, shouldAIProseParagraphBreak("Hello.", "World", 80))
	// Trailing hyphen = no break
	require.False(t, shouldAIProseParagraphBreak("contin-", "Uation", 80))
	// Trailing comma = no break
	require.False(t, shouldAIProseParagraphBreak("Hello,", "World", 80))
	// Empty next = no break
	require.False(t, shouldAIProseParagraphBreak("Hello.", "", 80))
}

func TestShouldAIProseParagraphBreak_NeedsSentenceEnd(t *testing.T) {
	require.False(t, shouldAIProseParagraphBreak("Hello", "World", 80))
}

func TestShouldAIProseParagraphBreak_WidePrev(t *testing.T) {
	// A wide prev (>= width-2) should suppress paragraph break
	long := "AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA AAAA ."
	require.False(t, shouldAIProseParagraphBreak(long, "World", 80))
}

func TestSplitProseParagraphs_EmptyInput(t *testing.T) {
	require.Empty(t, SplitProseParagraphs("", 80))
}

func TestSplitProseParagraphs_SeparatorLineBreaks(t *testing.T) {
	got := SplitProseParagraphs("Hello\n---\nWorld", 80)
	require.Len(t, got, 2)
}

func TestStripSyntax_NoSyntax(t *testing.T) {
	require.Equal(t, "hello world", StripSyntax("hello world"))
}

func TestStripSyntax_RemovesBold(t *testing.T) {
	require.Equal(t, "bold text", StripSyntax("**bold** text"))
}

func TestStripSyntax_RemovesItalic(t *testing.T) {
	require.Equal(t, "italic", StripSyntax("*italic*"))
}

func TestStripSyntax_RemovesInlineCode(t *testing.T) {
	require.Equal(t, "code", StripSyntax("`code`"))
}

func TestStripSyntax_RemovesLinks(t *testing.T) {
	require.Equal(t, "click here", StripSyntax("click [here](https://example.com)"))
}

func TestStripSyntax_RemovesHeadings(t *testing.T) {
	require.Equal(t, "Title", StripSyntax("# Title"))
	require.Equal(t, "Sub", StripSyntax("## Sub"))
}

func TestStripSyntax_RemovesImages(t *testing.T) {
	require.Equal(t, "alt text", StripSyntax("![alt text](img.png)"))
	require.Equal(t, "image", StripSyntax("![](img.png)"))
}

func TestStripSyntax_BoldWithinList(t *testing.T) {
	require.Equal(t, "- item", StripSyntax("- **item**"))
}

func TestFormatProse_EmptyInput(t *testing.T) {
	require.Equal(t, "", FormatProse("", 80))
}

func TestFormatProse_SingleLine(t *testing.T) {
	got := FormatProse("Hello world", 80)
	require.Contains(t, got, "Hello world")
}

func TestFormatProse_WithSeparator(t *testing.T) {
	got := FormatProse("Hello\n---\nWorld", 80)
	require.Contains(t, got, "Hello")
	require.Contains(t, got, "World")
}

func TestResetCache(t *testing.T) {
	// Just make sure it doesn't panic
	ResetCache()
}

// ─── Blockquote depth ──────────────────────────────────────────────────────

func TestBlockquoteDepth(t *testing.T) {
	d, ok := blockquoteDepth("> text")
	require.True(t, ok)
	require.Equal(t, 1, d)

	d, ok = blockquoteDepth("> > nested")
	require.True(t, ok)
	require.Equal(t, 2, d)

	_, ok = blockquoteDepth("not a quote")
	require.False(t, ok)

	_, ok = blockquoteDepth("")
	require.False(t, ok)
}

func TestNormalizeBlockquote_NoQuote(t *testing.T) {
	require.Equal(t, "plain", NormalizeBlockquote("plain"))
}

func TestNormalizeBlockquote_SingleDepth(t *testing.T) {
	in := "> Line 1\n> Line 2"
	got := NormalizeBlockquote(in)
	require.Equal(t, in, got)
}

func TestWrapHyperlink(t *testing.T) {
	got := wrapHyperlink("click here", "https://example.com")
	require.Contains(t, got, "click here")
}

func TestRenderSourcePreview(t *testing.T) {
	got := RenderSourcePreview(80, "line 1\n\nline 3")
	require.Contains(t, got, "line 1")
	require.Contains(t, got, "line 3")
}

// ─── FormatProse edge cases ────────────────────────────────────────────────

func TestFormatProse_ZeroWidth(t *testing.T) {
	got := FormatProse("Hello world", 0)
	require.Equal(t, "Hello world", got)
}
