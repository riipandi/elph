package runtime

import (
	"context"
	"errors"

	"github.com/riipandi/elph/pkg/skill"
)

const maxSkillBytes = 128 << 10

func executeSkill(ctx context.Context, workDir string, args map[string]any) ToolResult {
	name, ok := stringArg(args, "skill")
	if !ok {
		return ToolResult{Err: errors.New("missing required argument: skill")}
	}
	extra, _ := stringArg(args, "args")

	out, err := skill.Invoke(ctx, workDir, name, extra)
	if err != nil {
		return ToolResult{Err: err}
	}
	return ToolResult{Output: truncateToolOutput(out, maxSkillBytes)}
}
