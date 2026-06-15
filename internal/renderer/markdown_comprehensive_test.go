package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

const comprehensiveMarkdown = `# Heading 1

Paragraf biasa dengan **teks tebal**, *teks miring*, dan ~~teks coret~~. Juga ada ` + "`inline code`" + ` di dalam paragraf.

## Heading 2

Paragraf dengan [tautan](https://example.com) dan gambar ![alt text](https://via.placeholder.com/150). Perhatikan juga **_gabungan tebal dan miring_** serta ***bold italic***.

### Heading 3

Kutipan bertingkat:

> Kutipan level 1.
>
> > Kutipan level 2, atau kutipan bersarang.

#### Heading 4

- List tak berurut (item 1)
- List tak berurut (item 2)
  - List bersarang (item 2a)
  - List bersarang (item 2b)

##### Heading 5

1. List berurut (item 1)
2. List berurut (item 2)
   1. List berurut bersarang (item 2a)
   2. List berurut bersarang (item 2b)

###### Heading 6

Horizontal rule:

---

Task list:

- [ ] Belum selesai
- [ ] Juga belum selesai
- [x] Sudah selesai

### Code Blocks

Inline code: Gunakan ` + "`fmt.Println()`" + ` untuk mencetak output.

` + "```go\npackage main\n\nimport \"fmt\"\n\nfunc main() {\n    name := \"Elph\"\n    fmt.Printf(\"Hello, %s!\\n\", name)\n}\n```" + `

` + "```python\ndef greet(name: str) -> str:\n    return f\"Hello, {name}!\"\n```" + `

` + "```\nplain text code block\n```" + `

    this is an indented code block
    each line starts with 4 spaces

### Tabel

| Kolom A | Kolom B | Kolom C |
|---------|--------:|--------:|
| kiri    |   tengah|   kanan|
| foo     | bar     | baz    |

Ini referensi ke footnote[^1].

[^1]: Ini adalah isi footnote.

[link-ref]: https://example.com "Judul Tautan"
Kunjungi [situs ini][link-ref] untuk info lebih lanjut.

` + `\*bukan teks miring\* dan \` + "`bukan inline code`" + `\.

> - [x] Langkah pertama selesai
> - [ ] Langkah kedua belum
`

func plainContainsCollapsed(plain, want string) bool {
	return strings.Contains(strings.Join(strings.Fields(plain), " "), want)
}

func TestComprehensiveMarkdownRenders(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{text: comprehensiveMarkdown, kind: constants.MessageAI})
	plain := stripANSI(raw)

	checks := []string{
		"Heading 1", "Heading 2", "Heading 6",
		"teks tebal", "teks miring", "teks coret", "inline code",
		"tautan", "alt text",
		"gabungan tebal dan miring", "bold italic",
		"Kutipan level 1", "Kutipan level 2",
		"List tak berurut", "List bersarang",
		"List berurut",
		"fmt.Println", "package main", "def greet",
		"plain text code block", "indented code block",
		"Kolom A", "Kolom B", "kiri", "foo",
		"footnote", "situs ini",
		"bukan teks miring", "bukan inline code",
		"Langkah pertama",
	}
	for _, want := range checks {
		require.True(t, plainContainsCollapsed(plain, want), "missing: %s", want)
	}

	bads := []string{"**", "~~", "```", "]("}
	for _, bad := range bads {
		require.NotContains(t, plain, bad, "raw syntax leaked: %s", bad)
	}
}

func TestComprehensiveMarkdownDebugOutput(t *testing.T) {
	if testing.Short() {
		t.Skip("debug output")
	}
	m := testModel()
	plain := stripANSI(m.renderMessage(message{text: comprehensiveMarkdown, kind: constants.MessageAI}))
	t.Logf("\n%s\n", plain)
	_ = strings.TrimSpace(plain)
}
