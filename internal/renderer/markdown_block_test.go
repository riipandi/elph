package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

func TestPreprocessFootnotesRendersReadableNotes(t *testing.T) {
	in := "Lihat footnote[^1] di sini.\n\n[^1]: Catatan penting."
	got := preprocessFootnotes(in)
	require.Contains(t, got, "footnote(1)")
	require.Contains(t, got, "> [1] Catatan penting.")
	require.NotContains(t, got, "[^1]")
}

func TestPreprocessHTMLDetails(t *testing.T) {
	in := "<details><summary>Buka</summary>\n\nIsi tersembunyi.\n</details>"
	got := preprocessHTMLBlocks(in)
	require.Contains(t, got, "> **Buka**")
	require.Contains(t, got, "Isi tersembunyi.")
	require.NotContains(t, got, "<details>")
}

func TestNormalizeBlockquoteMarkdownClosesNestedDepth(t *testing.T) {
	in := "> Outer\n> > Nested\n> Still"
	got := normalizeBlockquoteMarkdown(in)
	require.Contains(t, got, "> > Nested\n>\n> Still")
}

func TestWideTableKeepsSingleRowPerLine(t *testing.T) {
	m := testModel()
	table := "| Trigger | Handler |\n|-------------------------|------------------------------------------------------------------------|\n| Normal chat input | runtime.Session.StartTurn → agent.RunTurn |"
	plain := stripANSI(m.renderMessage(message{text: table, kind: constants.MessageAI}))
	require.Contains(t, plain, "Trigger")
	require.Contains(t, plain, "Handler")
	require.Contains(t, plain, "│")
	// Wrapped table cells must not split "StartTurn" across a broken row marker.
	require.NotContains(t, plain, "StartTurn\n")
	lines := strings.Split(plain, "\n")
	dataRows := 0
	for _, line := range lines {
		if strings.Contains(line, "Normal chat input") {
			dataRows++
		}
	}
	require.Equal(t, 1, dataRows, "table data row should stay on one line")
}
