package runtime

import (
	"context"
	"errors"

	"github.com/riipandi/elph/pkg/tool/fetchurl"
)

func executeFetchURL(ctx context.Context, args map[string]any) ToolResult {
	rawURL, ok := stringArg(args, "url")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: url")}
	}
	result, err := fetchurl.Fetch(ctx, rawURL)
	if err != nil {
		return ToolResult{Err: err}
	}
	out := fetchurl.Format(result)
	return ToolResult{Output: truncateToolOutput(out, fetchurl.MaxBytes)}
}
