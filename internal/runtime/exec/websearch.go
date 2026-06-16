package exec

import (
	"context"
	"errors"
	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/pkg/tools/websearch"
)

const maxWebSearchBytes = 128 << 10

func executeWebSearch(ctx context.Context, args map[string]any) toolresult.ToolResult {
	query, ok := stringArg(args, "query")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: query")}
	}
	engine, _ := stringArg(args, "engine")
	limit := intArg(args, "limit", 5)
	includeContent := boolArg(args, "include_content")

	var opts []websearch.SearchOption
	if engine != "" {
		opts = append(opts, websearch.WithEngine(engine))
	}
	if limit > 0 {
		opts = append(opts, websearch.WithLimit(limit))
	}
	if includeContent {
		opts = append(opts, websearch.WithIncludeContent())
	}

	used, results, err := websearch.Search(ctx, query, opts...)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	out := websearch.Format(used, query, results)
	return toolresult.ToolResult{Output: truncateToolOutput(out, maxWebSearchBytes)}
}
