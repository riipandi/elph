package mediaimage

import (
	"bytes"
	"encoding/base64"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	"image/png"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/riipandi/elph/internal/projectdir"
	"golang.org/x/image/draw"
	_ "golang.org/x/image/webp"
)

const (
	MaxReadBytes        = 20 << 20
	MaxVisionDimension  = 1568
	MaxToolBase64Budget = 24 << 10
	MaxUserAttachBytes  = 5 << 20
	MaxUserAttachments  = 4
)

var (
	ErrUnsupportedMedia = errors.New("unsupported media type")
	ErrVideoUnsupported = errors.New("video files are not supported yet")
)

// ReadPath reads an image file and returns normalized PNG bytes for vision APIs.
func ReadPath(path string) (data []byte, mime string, width, height int, err error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, "", 0, 0, err
	}
	if len(raw) > MaxReadBytes {
		return nil, "", 0, 0, fmt.Errorf("file exceeds %d byte limit", MaxReadBytes)
	}
	return Normalize(raw, sniffMIME(path, raw))
}

// Normalize decodes an image, optionally downscales, and re-encodes as PNG.
func Normalize(data []byte, mimeHint string) ([]byte, string, int, int, error) {
	mime := strings.TrimSpace(mimeHint)
	if mime == "" {
		mime = http.DetectContentType(data)
	}
	if strings.HasPrefix(mime, "video/") {
		return nil, "", 0, 0, ErrVideoUnsupported
	}
	if !strings.HasPrefix(mime, "image/") {
		return nil, "", 0, 0, fmt.Errorf("%w: %s", ErrUnsupportedMedia, mime)
	}

	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return nil, "", 0, 0, err
	}
	bounds := img.Bounds()
	width, height := bounds.Dx(), bounds.Dy()
	img = downscale(img, MaxVisionDimension)
	bounds = img.Bounds()
	width, height = bounds.Dx(), bounds.Dy()

	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		return nil, "", 0, 0, err
	}
	out := buf.Bytes()
	if len(out) > MaxUserAttachBytes {
		return nil, "", 0, 0, fmt.Errorf("image exceeds %d byte limit after resize", MaxUserAttachBytes)
	}
	return out, "image/png", width, height, nil
}

// FormatToolResult builds ReadMediaFile tool output with metadata and base64 PNG.
func FormatToolResult(relPath, mime string, width, height int, data []byte) string {
	payload, truncated := base64ForTool(data)
	var b strings.Builder
	fmt.Fprintf(&b, "path: %s\nmime: %s\nwidth: %d\nheight: %d\nsize_bytes: %d\n", relPath, mime, width, height, len(data))
	if truncated {
		b.WriteString("note: image resized/recompressed to fit tool output limit\n")
	}
	b.WriteString("data_base64: ")
	b.WriteString(payload)
	return b.String()
}

// SaveAttachment writes PNG bytes under <workDir>/.agents/elph/attachments/.
func SaveAttachment(workDir, sessionSuffix string, data []byte) (absPath, relPath string, err error) {
	if err := projectdir.EnsureRoot(workDir); err != nil {
		return "", "", err
	}
	dir := projectdir.AttachmentsDir(workDir)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", "", err
	}
	name := fmt.Sprintf("paste_%s_%d.png", sessionSuffix, time.Now().UnixNano())
	for i := 0; i < 1000; i++ {
		if i > 0 {
			name = fmt.Sprintf("paste_%s_%d_%d.png", sessionSuffix, time.Now().UnixNano(), i)
		}
		absPath = filepath.Join(dir, name)
		if _, statErr := os.Stat(absPath); os.IsNotExist(statErr) {
			break
		}
	}
	if err := os.WriteFile(absPath, data, 0o644); err != nil {
		return "", "", err
	}
	rel, err := filepath.Rel(workDir, absPath)
	if err != nil {
		rel = absPath
	}
	return absPath, rel, nil
}

func sniffMIME(path string, data []byte) string {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".png":
		return "image/png"
	case ".jpg", ".jpeg":
		return "image/jpeg"
	case ".gif":
		return "image/gif"
	case ".webp":
		return "image/webp"
	}
	return http.DetectContentType(data)
}

func downscale(img image.Image, maxDim int) image.Image {
	bounds := img.Bounds()
	w, h := bounds.Dx(), bounds.Dy()
	if w <= maxDim && h <= maxDim {
		return img
	}
	scale := float64(maxDim) / float64(max(w, h))
	nw := max(1, int(float64(w)*scale))
	nh := max(1, int(float64(h)*scale))
	dst := image.NewRGBA(image.Rect(0, 0, nw, nh))
	draw.CatmullRom.Scale(dst, dst.Bounds(), img, bounds, draw.Over, nil)
	return dst
}

func base64ForTool(data []byte) (string, bool) {
	if len(data) == 0 {
		return "", false
	}
	encoded := base64.StdEncoding.EncodeToString(data)
	if len(encoded) <= MaxToolBase64Budget {
		return encoded, false
	}
	// Recompress more aggressively for tool output.
	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return truncateBase64(encoded), true
	}
	for _, dim := range []int{1024, 768, 512, 384, 256} {
		scaled := downscale(img, dim)
		var buf bytes.Buffer
		if err := png.Encode(&buf, scaled); err != nil {
			continue
		}
		encoded = base64.StdEncoding.EncodeToString(buf.Bytes())
		if len(encoded) <= MaxToolBase64Budget {
			return encoded, true
		}
	}
	return truncateBase64(encoded), true
}

func truncateBase64(encoded string) string {
	if len(encoded) <= MaxToolBase64Budget {
		return encoded
	}
	return encoded[:MaxToolBase64Budget]
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// DecodeConfig reports image dimensions without full decode when possible.
func DecodeConfig(r io.Reader) (width, height int, err error) {
	cfg, _, err := image.DecodeConfig(r)
	if err != nil {
		return 0, 0, err
	}
	return cfg.Width, cfg.Height, nil
}
