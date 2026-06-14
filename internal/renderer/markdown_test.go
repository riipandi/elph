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
	require.True(t, m.messages[0].renderCache.hit(m.messageAreaWidth(), false, len(m.messages[0].text)))
}

func TestLargeMarkdownSchedulesAsyncGlamour(t *testing.T) {
	m := testModel()
	body := "## Report\n\n" + strings.Repeat("detail line with **bold** text.\n", 80)
	m.messages = []message{{text: body, kind: constants.MessageAI}}

	updated, cmd := m.scheduleGlamourRender(0)
	require.NotNil(t, cmd)
	require.True(t, updated.messages[0].glamourPending)

	preview := stripANSI(updated.renderMessageAt(0))
	require.Contains(t, preview, "## Report")
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
	require.True(t, updated.messages[0].renderCache.hit(m.messageAreaWidth(), false, len(source)))
}

func TestAIMarkdownPreservesBlockWidth(t *testing.T) {
	m := testModel()
	rendered := m.renderMessage(message{
		text: "# Title\n\nA longer markdown paragraph that should wrap inside the message block.",
		kind: constants.MessageAI,
	})
	require.LessOrEqual(t, lipgloss.Width(rendered), m.messageAreaWidth())
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