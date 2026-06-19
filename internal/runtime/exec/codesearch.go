package exec

import (
	"context"
	"errors"

	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/pkg/tools/codesearch"
)

const maxCodeSearchBytes = 128 << 10

func executeCodeSearch(ctx context.Context, args map[string]any) toolresult.ToolResult {
	query, ok := stringArg(args, "query")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: query")}
	}
	provider, _ := stringArg(args, "provider")

	used, results, err := codesearch.Search(ctx, query, provider)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	out := codesearch.Format(used, query, results)
	return toolresult.ToolResult{Output: truncateToolOutput(out, maxCodeSearchBytes)}
}
