package renderer

import (
	"fmt"
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
)

func TestReproRemainingBugs(t *testing.T) {
	m := testModel()
	cases := []struct {
		name string
		md   string
	}{
		{"bold_italic", "***teks tebal miring***"},
		{"bold_italic_prose", "Ini ***teks tebal miring*** di kalimat."},
		{"link", "Kunjungi [GitHub](https://github.com) sekarang."},
		{"image", "Lihat ![logo](https://example.com/logo.png) di sini."},
		{"table", "| A | B |\n|---|---|\n| 1 | 2 |"},
		{"table_prose", "Tabel:\n\n| A | B |\n|---|---|\n| 1 | 2 |"},
		{"nested_quote", "> Luar\n> > Dalam\n> Kembali luar"},
		{"mixed", "> Kutipan\n\n***tebal miring*** dan [link](https://x.com)\n\n| X | Y |\n|---|---|\n| a | b |"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			plain := stripANSI(m.renderMessage(message{text: c.md, kind: constants.MessageAI}))
			block := hasMarkdownBlockStructure(c.md)
			t.Logf("block=%v", block)
			t.Logf("OUT:\n%s", plain)
			fmt.Printf("\n=== %s (block=%v) ===\n%s\n", c.name, block, strings.TrimSpace(plain))
		})
	}
}
