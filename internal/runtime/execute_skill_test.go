package runtime

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/pkg/skill"
	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func writeTestSkill(t *testing.T, dir, content string) {
	t.Helper()
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, skill.FileName), []byte(content), 0o644))
}

func TestExecuteSkill(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeTestSkill(t, filepath.Join(home, ".elph", "skills", "review"), `---
name: review
description: Code review workflow
type: inline
---
## Review steps
1. Read diff`)

	ctx := skill.WithDepthHolder(context.Background())
	result := ExecuteTool(ctx, workDir, tool.Skill, map[string]any{
		"skill": "review",
		"args":  "focus on security",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, `<skill_content name="review">`)
	require.Contains(t, result.Output, "## Review steps")
	require.Contains(t, result.Output, "<user_args>\nfocus on security\n</user_args>")
}

func TestExecuteSkillRejectsUnknown(t *testing.T) {
	result := ExecuteTool(context.Background(), t.TempDir(), tool.Skill, map[string]any{
		"skill": "missing",
	})
	require.Error(t, result.Err)
}

func TestExecuteSkillRejectsMaxDepth(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeTestSkill(t, filepath.Join(home, ".elph", "skills", "a"), `---
name: a
description: A
type: inline
---
A`)

	ctx := skill.WithDepthHolder(context.Background())
	for range skill.MaxNestingDepth {
		require.NoError(t, skill.Enter(ctx))
	}
	result := ExecuteTool(ctx, workDir, tool.Skill, map[string]any{"skill": "a"})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "nesting depth")
}
