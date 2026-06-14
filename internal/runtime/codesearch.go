package runtime

import (
	"context"
	"errors"

	"github.com/riipandi/elph/pkg/tool/codesearch"
)

const maxCodeSearchBytes = 128 << 10

func executeCodeSearch(ctx context.Context, args map[string]any) ToolResult {
	query, ok := stringArg(args, "query")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: query")}
	}
	provider, _ := stringArg(args, "provider")

	used, results, err := codesearch.Search(ctx, query, provider)
	if err != nil {
		return ToolResult{Err: err}
	}
	out := codesearch.Format(used, query, results)
	return ToolResult{Output: truncateToolOutput(out, maxCodeSearchBytes)}
}
