package runtime

import (
	"context"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestExecuteReadMediaFilePNG(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "shot.png")
	writeTestPNG(t, path, 32, 24)

	result := ExecuteTool(context.Background(), dir, tool.ReadMediaFile, map[string]any{
		"path": "shot.png",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "path: shot.png")
	require.Contains(t, result.Output, "mime: image/png")
	require.Contains(t, result.Output, "data_base64:")
}

func TestExecuteReadMediaFileRejectsVideo(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	path := filepath.Join(dir, "clip.mp4")
	require.NoError(t, os.WriteFile(path, []byte{0, 0, 0, 0x18, 'f', 't', 'y', 'p'}, 0o644))

	result := ExecuteTool(context.Background(), dir, tool.ReadMediaFile, map[string]any{
		"path": "clip.mp4",
	})
	require.Error(t, result.Err)
}

func writeTestPNG(t *testing.T, path string, w, h int) {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	f, err := os.Create(path)
	require.NoError(t, err)
	defer f.Close()
	require.NoError(t, png.Encode(f, img))
}
