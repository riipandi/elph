package exec

import (
	"context"
	"errors"

	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/pkg/tools/fetchurl"
)

func executeFetchURL(ctx context.Context, args map[string]any) toolresult.ToolResult {
	rawURL, ok := stringArg(args, "url")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: url")}
	}
	result, err := fetchurl.Fetch(ctx, rawURL)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	out := fetchurl.Format(result)
	return toolresult.ToolResult{Output: truncateToolOutput(out, fetchurl.MaxBytes)}
}
