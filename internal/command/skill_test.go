package command

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/pkg/skill"
	"github.com/stretchr/testify/require"
)

func writeSkillDir(t *testing.T, dir, content string) {
	t.Helper()
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, skill.FileName), []byte(content), 0o644))
}

func TestExecuteSkillSlashCommand(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkillDir(t, filepath.Join(home, ".elph", "skills", "code-review"), `---
name: code-review
description: Review code changes
type: inline
---
## Steps
1. Read the diff`)

	ctx := Context{
		WorkDir: workDir,
		Skills:  LoadSlashSkills(workDir),
	}

	result := Execute("/skill:code-review focus on security", ctx)
	require.True(t, result.OK)
	require.Contains(t, result.AgentPrompt, `<skill_content name="code-review">`)
	require.Contains(t, result.AgentPrompt, "## Steps")
	require.Contains(t, result.AgentPrompt, "<user_args>\nfocus on security\n</user_args>")
	require.NotContains(t, result.AgentPrompt, "skill: code-review")
	require.Equal(t, "Skill: code-review", result.DetailLabel)
	require.Contains(t, result.DetailBody, "## Steps")
	require.Contains(t, result.DetailBody, "<user_args>\nfocus on security\n</user_args>")
	require.Equal(t, result.AgentPrompt, result.DetailBody)
	require.False(t, result.DetailExpanded)
	require.NotContains(t, result.DetailBody, "skill: code-review")
	require.Empty(t, result.Output)
}

func TestAllIncludesSkillSlashCommands(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkillDir(t, filepath.Join(home, ".elph", "skills", "deploy"), `---
name: deploy
description: Deploy workflow
---
Deploy`)

	ctx := Context{
		WorkDir: workDir,
		Skills:  LoadSlashSkills(workDir),
	}

	names := make([]string, len(All(ctx)))
	for i, cmd := range All(ctx) {
		names[i] = cmd.Name
	}
	require.Contains(t, names, "skill:deploy")
}

func TestSuggestSkillSlashCommands(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkillDir(t, filepath.Join(home, ".elph", "skills", "pdf-tools"), `---
name: pdf-tools
description: Work with PDF files
---
PDF`)

	ctx := Context{
		WorkDir: workDir,
		Skills:  LoadSlashSkills(workDir),
	}

	got := Suggest("skill:pdf", ctx)
	require.NotEmpty(t, got)
	require.Equal(t, "skill:pdf-tools", got[0].Name)
}

func TestSkillSlashIncludesDisableModelInvocation(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkillDir(t, filepath.Join(home, ".elph", "skills", "manual"), `---
name: manual
description: Manual only skill
disable-model-invocation: true
---
Manual`)

	skills := LoadSlashSkills(workDir)
	require.Len(t, skills, 1)

	result := Execute("/skill:manual", Context{WorkDir: workDir, Skills: skills})
	require.True(t, result.OK)
	require.Contains(t, result.AgentPrompt, "Manual")
	require.Equal(t, "Skill: manual", result.DetailLabel)
	require.Contains(t, result.DetailBody, "Manual")
}
