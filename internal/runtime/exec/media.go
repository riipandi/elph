package exec

import (
	"errors"
	"fmt"
	"github.com/riipandi/elph/internal/runtime/toolresult"
	"path/filepath"

	"github.com/riipandi/elph/internal/mediaimage"
)

func executeReadMediaFile(workDir string, args map[string]any) toolresult.ToolResult {
	path, ok := stringArg(args, "path")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: path")}
	}
	full, err := resolveWorkPath(workDir, path)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	if dir, dirErr := checkIsDirectory(full); dirErr == nil && dir {
		return toolresult.ToolResult{
			Err: fmt.Errorf("%s is a directory — use ReadMediaFile on a specific file, not a directory", path),
		}
	}
	data, mime, width, height, err := mediaimage.ReadPath(full)
	if err != nil {
		if errors.Is(err, mediaimage.ErrVideoUnsupported) {
			return toolresult.ToolResult{Err: err}
		}
		return toolresult.ToolResult{Err: fmt.Errorf("read media file: %w", err)}
	}
	rel := path
	if workDir != "" {
		if r, relErr := filepath.Rel(workDir, full); relErr == nil {
			rel = r
		}
	}
	rel = filepath.ToSlash(rel)
	output := mediaimage.FormatToolResult(rel, mime, width, height, data)
	return toolresult.ToolResult{Output: truncateToolOutput(output, maxMediaToolBytes)}
}

const maxMediaToolBytes = 32 << 10
