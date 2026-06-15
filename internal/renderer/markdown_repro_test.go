package renderer

import (
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestMarkdownFeatureRegression(t *testing.T) {
	m := testModel()
	cases := []struct {
		name       string
		text       string
		contains   []string
		notContain []string
	}{
		{
			name:       "link_inline",
			text:       "Visit [GitHub](https://github.com/riipandi/elph) for more.",
			contains:   []string{"GitHub"},
			notContain: []string{"]("},
		},
		{
			name:       "strike_inline",
			text:       "This is ~~removed~~ text.",
			contains:   []string{"removed"},
			notContain: []string{"~~"},
		},
		{
			name:       "blockquote",
			text:       "> Line one\n> Line two\n> Line three",
			contains:   []string{"Line one", "Line two", "Line three"},
			notContain: []string{},
		},
		{
			name:       "nested_blockquote",
			text:       "> Outer quote\n> > Nested quote\n> Still outer",
			contains:   []string{"Outer quote", "Nested quote", "Still outer"},
			notContain: []string{"> Nested"},
		},
		{
			name:       "table",
			text:       "| Col A | Col B |\n|-------|-------|\n| one   | two   |",
			contains:   []string{"Col A", "Col B", "one", "two", "│"},
			notContain: []string{"|---|---|"},
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			raw := m.renderMessage(message{text: c.text, kind: constants.MessageAI})
			plain := stripANSI(raw)
			for _, want := range c.contains {
				require.Contains(t, plain, want, "complete path")
			}
			for _, bad := range c.notContain {
				require.NotContains(t, plain, bad, "complete path plain")
			}
		})
	}
}

func TestStreamingBlockPreviewPreservesTableRows(t *testing.T) {
	m := testModel()
	m.agent.Busy = true
	m.agent.ResponseMsgID = 0
	table := "| Col A | Col B |\n|-------|-------|\n| one   | two   |"
	m.messages = []message{{text: table, kind: constants.MessageAI}}

	plain := stripANSI(m.renderMessageAt(0))
	require.Contains(t, plain, "Col A")
	require.Contains(t, plain, "Col B")
	require.Contains(t, plain, "one")
	require.Contains(t, plain, "two")
	require.Contains(t, plain, "|")
}
