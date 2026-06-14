package mediaimage

import (
	"bytes"
	"image"
	"image/png"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizePNG(t *testing.T) {
	t.Parallel()

	img := image.NewRGBA(image.Rect(0, 0, 8, 6))
	var raw bytes.Buffer
	require.NoError(t, png.Encode(&raw, img))

	data, mime, w, h, err := Normalize(raw.Bytes(), "image/png")
	require.NoError(t, err)
	require.Equal(t, "image/png", mime)
	require.Equal(t, 8, w)
	require.Equal(t, 6, h)
	require.NotEmpty(t, data)
}

func TestFormatToolResultIncludesBase64(t *testing.T) {
	t.Parallel()

	out := FormatToolResult(".agents/elph/attachments/a.png", "image/png", 4, 4, []byte{1, 2, 3})
	require.Contains(t, out, "path: .agents/elph/attachments/a.png")
	require.Contains(t, out, "data_base64:")
}
