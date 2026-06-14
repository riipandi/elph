package runtime

import (
	"context"

	"github.com/riipandi/elph/pkg/memz"
)

func executeTodoList(ctx context.Context, args map[string]any) ToolResult {
	raw, present := args["todos"]
	out, err := memz.Apply(ctx, raw, present)
	if err != nil {
		return ToolResult{Err: err}
	}
	return ToolResult{Output: out}
}
