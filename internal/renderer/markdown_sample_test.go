package renderer

import (
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/constants"
	"github.com/stretchr/testify/require"
)

const userSampleMarkdown = `# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6

---

## Teks Dasar

Ini adalah paragraf biasa. **Teks tebal** dan *teks miring* dan ~~teks coret~~ dan **_teks tebal miring_**.

Ini adalah paragraf kedua dengan ` + "`inline code`" + ` di dalamnya.

> Ini adalah blockquote.
> > Dan ini blockquote bersarang.

## Daftar

### Unordered List
- Item 1
  - Sub-item 1a
  - Sub-item 1b
- Item 2
- Item 3

### Ordered List
1. Langkah pertama
2. Langkah kedua
3. Langkah ketiga

### Task List
- [x] Selesai
- [ ] Belum selesai
- [ ] Belum selesai lagi

## Tabel

| Nama    | Usia | Kota        |
|---------|------|-------------|
| Alice   | 30   | Jakarta     |
| Bob     | 25   | Bandung     |
| Charlie | 35   | Surabaya    |

Alignment juga bisa diatur:

| Kiri | Tengah | Kanan |
|:-----|:------:|------:|
| L    |   C    |     R |

## Links dan Images

[Google](https://www.google.com) — link biasa.

[Link dengan judul](https://www.google.com "judul tooltip") — link dengan title attribute.

Ini referensi internal ke [heading](#heading-1) di atas.

![Alt text untuk gambar](https://via.placeholder.com/150 "Judul gambar")

## Code Block

### Inline

Gunakan ` + "`fmt.Println()`" + ` untuk cetak output.

### Fenced Code Block (tanpa bahasa)

` + "```" + `
ini code block tanpa syntax highlighting
baris kedua
` + "```" + `

### Fenced Code Block (dengan bahasa)

` + "```go" + `
package main

import "fmt"

func main() {
    name := "World"
    fmt.Printf("Hello, %s!\n", name)
}
` + "```" + `

` + "```python" + `
def fibonacci(n: int) -> list[int]:
    """Generate fibonacci sequence."""
    fib = [0, 1]
    for i in range(2, n):
        fib.append(fib[i-1] + fib[i-2])
    return fib[:n]

print(fibonacci(10))
` + "```" + `

` + "```json" + `
{
  "name": "elph",
  "version": "1.0.0",
  "dependencies": {
    "golang": ">=1.22"
  }
}
` + "```" + `

` + "```bash" + `
# Build the project
make build

# Run tests
make test

# Lint check
make lint
` + "```" + `

### Indented Code (4 spasi)

    ini code block dengan indentasi
    baris kedua
    baris ketiga

## Escape Characters

\*ini bukan bold\* — karakter di-escape dengan backslash.

## Horizontal Rule

---

***

___

## Footnotes

Ini adalah referensi ke footnote[^1] dan footnote lainnya[^note].

[^1]: Ini footnote pertama.
[^note]: Ini footnote dengan nama.

## Definition List

Term 1
:   Definisi dari Term 1

Term 2
:   Definisi dari Term 2
:   Definisi alternatif dari Term 2

## Abbreviations

The HTML specification is maintained by the W3C.

*[HTML]: Hyper Text Markup Language
*[W3C]: World Wide Web Consortium

## Emoji

:smile: :rocket: :check_mark: :warning:

## Math (jika didukung)

Inline math: $E = mc^2$

Block math:

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

## Nested Blockquote dalam List

1. Langkah pertama
   > Ini blockquote di dalam list item.
   > Bisa multi-baris juga.

2. Langkah kedua

## Table dengan Code di Dalam

| Perintah | Deskripsi                |
|----------|--------------------------|
| ` + "`ls`" + `     | List directory           |
| ` + "`cd`" + `     | Change directory         |
| ` + "`rm -rf`" + ` | Hapus secara rekursif ⚠️ |

## HTML di Markdown

<details>
<summary>Klik untuk expand</summary>

Ini konten yang tersembunyi.

` + "```go" + `
fmt.Println("dalam details")
` + "```" + `

</details>
`

func TestUserSampleMarkdownRenders(t *testing.T) {
	m := testModel()
	raw := m.renderMessage(message{text: userSampleMarkdown, kind: constants.MessageAI})
	plain := stripANSI(raw)

	// Headings
	for _, h := range []string{"Heading 1", "Heading 2", "Heading 6", "Teks Dasar", "Daftar"} {
		require.Contains(t, plain, h, "heading %s", h)
	}

	// Inline text
	for _, s := range []string{
		"Teks tebal", "teks miring", "teks coret", "teks tebal miring", "inline code",
	} {
		require.Contains(t, plain, s, "inline %s", s)
	}

	// Blockquote
	require.Contains(t, plain, "Ini adalah blockquote")
	require.Contains(t, plain, "blockquote bersarang")
	require.NotContains(t, plain, "blockquote. Dan ini")

	// Lists
	for _, s := range []string{"Item 1", "Sub-item 1a", "Langkah pertama", "Selesai", "Belum selesai"} {
		require.Contains(t, plain, s, "list %s", s)
	}

	// Tables
	for _, s := range []string{"Nama", "Alice", "Jakarta", "Kiri", "Tengah", "Kanan", "Perintah", "ls"} {
		require.Contains(t, plain, s, "table %s", s)
	}
	require.Contains(t, plain, "│")

	// Links / images
	require.Contains(t, plain, "Google")
	require.NotContains(t, plain, "https://www.google.com")
	require.Contains(t, plain, "Alt text untuk gambar")
	require.NotContains(t, plain, "placeholder.com")

	// Code
	for _, s := range []string{"fmt.Println", "package main", "fibonacci", "make build", "indentasi"} {
		require.Contains(t, plain, s, "code %s", s)
	}

	// Escape
	require.Contains(t, plain, "ini bukan bold")

	// Horizontal rules (Glamour renders --- as --------)
	require.Contains(t, plain, "--------")

	// Footnotes preprocessed to readable notes
	require.Contains(t, plain, "footnote pertama")
	require.Contains(t, plain, "footnote dengan nama")
	require.NotContains(t, plain, "[^1]")
	require.NotContains(t, plain, "[^note]:")

	// Definition list via glamour
	require.Contains(t, plain, "Definisi dari Term 1")

	// Abbreviation definition lines stripped (TUI has no hover)
	require.NotContains(t, plain, "*[HTML]:")

	// Emoji shortcodes (best-effort; not all aliases are supported)
	require.True(t, strings.Contains(plain, "😄") || strings.Contains(plain, ":smile:"))

	// HTML details collapsed to blockquote-style summary
	require.Contains(t, plain, "Klik untuk expand")
	require.Contains(t, plain, "konten yang tersembunyi")

	// Nested blockquote in list
	require.Contains(t, plain, "blockquote di dalam list")

	// Math stays literal (unsupported in glamour)
	require.Contains(t, plain, "E = mc")

	// No raw syntax leaks
	bads := []string{"**", "~~", "```", "]("}
	for _, bad := range bads {
		require.NotContains(t, plain, bad, "leaked %s", bad)
	}
}

func TestUserSampleMarkdownDebug(t *testing.T) {
	if testing.Short() {
		t.Skip("debug output")
	}
	m := testModel()
	plain := stripANSI(m.renderMessage(message{text: userSampleMarkdown, kind: constants.MessageAI}))
	t.Log("\n" + plain)
	_ = strings.TrimSpace(plain)
}
