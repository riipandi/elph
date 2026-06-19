package exec

import (
	"context"
	"errors"

	"github.com/riipandi/elph/internal/runtime/toolresult"

	"github.com/riipandi/elph/pkg/skill"
)

const maxSkillBytes = 128 << 10

func executeSkill(ctx context.Context, workDir string, args map[string]any) toolresult.ToolResult {
	name, ok := stringArg(args, "skill")
	if !ok {
		return toolresult.ToolResult{Err: errors.New("missing required argument: skill")}
	}
	extra, _ := stringArg(args, "args")

	out, err := skill.Invoke(ctx, workDir, name, extra)
	if err != nil {
		return toolresult.ToolResult{Err: err}
	}
	return toolresult.ToolResult{Output: truncateToolOutput(out, maxSkillBytes)}
}
