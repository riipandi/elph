package runtime

import (
	"errors"
	"fmt"
	"path/filepath"

	"github.com/riipandi/elph/internal/mediaimage"
)

func executeReadMediaFile(workDir string, args map[string]any) ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: path")}
	}
	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return ToolResult{Err: err}
	}
	data, mime, width, height, err := mediaimage.ReadPath(full)
	if err != nil {
		if errors.Is(err, mediaimage.ErrVideoUnsupported) {
			return ToolResult{Err: err}
		}
		return ToolResult{Err: fmt.Errorf("read media file: %w", err)}
	}
	rel := path
	if workDir != "" {
		if r, relErr := filepath.Rel(workDir, full); relErr == nil {
			rel = r
		}
	}
	rel = filepath.ToSlash(rel)
	output := mediaimage.FormatToolResult(rel, mime, width, height, data)
	return ToolResult{Output: truncateToolOutput(output, maxMediaToolBytes)}
}

const maxMediaToolBytes = 32 << 10
