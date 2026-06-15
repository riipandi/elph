package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestGlamourH1MatchesOtherHeadings(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{
		text: "# Title one\n\n## Title two",
		kind: constants.MessageAI,
	})
	require.NotContains(t, raw, "48;2;") // no H1 badge background from default dark style
	require.Contains(t, stripANSI(raw), "Title one")
	require.Contains(t, stripANSI(raw), "Title two")
}

func TestGlamourLinkPreprocessHidesURL(t *testing.T) {
	m := testModel()
	plain := stripANSI(m.renderMessage(message{
		text: "Visit [GitHub](https://github.com) now.",
		kind: constants.MessageAI,
	}))
	require.Contains(t, plain, "GitHub")
	require.NotContains(t, plain, "https://github.com")
}

func TestGlamourImagePreprocessShowsAltOnly(t *testing.T) {
	m := testModel()
	plain := stripANSI(m.renderMessage(message{
		text: "See ![logo](https://example.com/logo.png) here.",
		kind: constants.MessageAI,
	}))
	require.Contains(t, plain, "logo")
	require.NotContains(t, plain, "Image:")
	require.NotContains(t, plain, "logo.png")
}

func TestCopyHintOnCachedAsyncMarkdown(t *testing.T) {
	m := testModel()
	source := "## Done\n\nAll good."
	m.messages = []message{{text: source, kind: constants.MessageAI}}
	updated, cmd := m.scheduleMarkdownRender(0)
	require.NotNil(t, cmd)
	rendered := cmd().(markdownRenderMsg)
	final, _ := updated.handleMarkdownRenderMsg(rendered)
	plain := stripANSI(final.renderMessageAt(0))
	require.True(t, strings.Contains(plain, aiCopyHintText), plain)
}
