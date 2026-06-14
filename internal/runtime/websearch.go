package runtime

import (
	"context"
	"errors"

	"github.com/riipandi/elph/pkg/tool/websearch"
)

const maxWebSearchBytes = 128 << 10

func executeWebSearch(ctx context.Context, args map[string]any) ToolResult {
	query, ok := stringArg(args, "query")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: query")}
	}
	engine, _ := stringArg(args, "engine")

	used, results, err := websearch.Search(ctx, query, engine)
	if err != nil {
		return ToolResult{Err: err}
	}
	out := websearch.Format(used, query, results)
	return ToolResult{Output: truncateToolOutput(out, maxWebSearchBytes)}
}
