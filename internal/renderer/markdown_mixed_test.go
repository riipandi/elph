package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestMixedMarkdownRendersInlineAndBlocks(t *testing.T) {
	m := testModel()
	md := "> Kutipan\n\n***tebal miring*** dan [link](https://x.com)\n\n| X | Y |\n|---|---|\n| a | b |"
	raw := m.renderMessage(message{text: md, kind: constants.MessageAI})
	plain := stripANSI(raw)

	require.Contains(t, plain, "Kutipan")
	require.Contains(t, plain, "tebal miring")
	require.Contains(t, plain, "link")
	require.Contains(t, plain, "X")
	require.Contains(t, plain, "Y")
	require.NotContains(t, plain, "***")
	require.NotContains(t, plain, "](")
	require.NotContains(t, plain, "|---|")
	require.True(t, strings.Contains(raw, "1;3m") || strings.Contains(raw, "1m") || strings.Contains(raw, "3m"))
}

func TestImageLinkInline(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{
		text: "Lihat ![logo](https://example.com/logo.png) di sini.",
		kind: constants.MessageAI,
	})
	plain := stripANSI(raw)
	require.Contains(t, plain, "logo")
	require.NotContains(t, plain, "![")
	require.NotContains(t, plain, "](")
}

func TestTableProseRendersTogether(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{
		text: "Tabel:\n\n| A | B |\n|---|---|\n| 1 | 2 |",
		kind: constants.MessageAI,
	})
	plain := stripANSI(raw)
	require.Contains(t, plain, "Tabel:")
	require.Contains(t, plain, "A")
	require.Contains(t, plain, "B")
	require.Contains(t, plain, "│")
}
