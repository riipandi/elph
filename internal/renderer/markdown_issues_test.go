package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestInlineStylesComplete(t *testing.T) {
	m := testModel()
	cases := map[string]string{
		"bold_italic": "***bold italic***",
		"strike":      "~~removed~~",
		"link":        "Visit [GitHub](https://github.com) here",
	}
	for name, md := range cases {
		t.Run(name, func(t *testing.T) {
			raw := m.renderMessage(message{text: md, kind: constants.MessageAI})
			plain := stripANSI(raw)
			require.NotContains(t, plain, "**")
			require.NotContains(t, plain, "~~")
			require.NotContains(t, plain, "](")
			switch name {
			case "bold_italic":
				require.Contains(t, plain, "bold italic")
				require.True(t, strings.Contains(raw, "1;3m") || (strings.Contains(raw, "1m") && strings.Contains(raw, "3m")))
			case "strike":
				require.Contains(t, plain, "removed")
				require.True(t, strings.Contains(raw, ";9m") || strings.Contains(raw, "[9m"))
			case "link":
				require.Contains(t, plain, "GitHub")
				require.NotContains(t, plain, "https://github.com")
			}
		})
	}
}

func TestBlockStructuresUseMarkdownRenderer(t *testing.T) {
	m := testModel()
	cases := map[string][]string{
		"table":        {"Col A", "Col B", "one", "two", "│"},
		"nested_quote": {"Outer quote", "Nested quote", "Still outer"},
	}
	for name, wants := range cases {
		t.Run(name, func(t *testing.T) {
			var md string
			switch name {
			case "table":
				md = "| Col A | Col B |\n|-------|-------|\n| one   | two   |"
			case "nested_quote":
				md = "> Outer quote\n> > Nested quote\n> Still outer"
			}
			plain := stripANSI(m.renderMessage(message{text: md, kind: constants.MessageAI}))
			for _, w := range wants {
				require.Contains(t, plain, w)
			}

			if name == "table" {
				require.NotContains(t, plain, "|---|")
			}
		})
	}
}

func TestStreamingKeepsRawMarkdownUntilComplete(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.agent.ResponseMsgID = 0
	m.messages = []message{{text: "***live***", kind: constants.MessageAI}}
	raw := m.renderMessageAt(0)
	require.Contains(t, stripANSI(raw), "***live***")
}
