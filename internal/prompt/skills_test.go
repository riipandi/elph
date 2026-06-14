package prompt

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestDiscoverSkillsFromGlobalAndProject(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	globalDir := filepath.Join(home, ".elph", "skills", "help")
	require.NoError(t, os.MkdirAll(globalDir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(globalDir, "SKILL.md"), []byte(`---
name: help
description: Documentation help
---
# Help
`), 0o644))

	projectDir := filepath.Join(workDir, ".agents", "elph", "skills", "review")
	require.NoError(t, os.MkdirAll(projectDir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(projectDir, "SKILL.md"), []byte(`---
name: review
description: Code review workflow
---
`), 0o644))

	skills := DiscoverSkills(workDir)
	require.Len(t, skills, 2)
	require.Equal(t, "help", skills[0].Name)
	require.Equal(t, "Documentation help", skills[0].Description)
	require.Contains(t, skills[0].Location, "help/SKILL.md")
	require.Equal(t, "review", skills[1].Name)
}

func TestDiscoverSkillsProjectOverridesGlobal(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	for _, scope := range []string{
		filepath.Join(home, ".elph", "skills", "deploy"),
		filepath.Join(workDir, ".agents", "elph", "skills", "deploy"),
	} {
		require.NoError(t, os.MkdirAll(scope, 0o755))
	}

	require.NoError(t, os.WriteFile(filepath.Join(home, ".elph", "skills", "deploy", "SKILL.md"), []byte(`---
name: deploy
description: Global deploy skill
---
`), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(workDir, ".agents", "elph", "skills", "deploy", "SKILL.md"), []byte(`---
name: deploy
description: Project deploy skill
---
`), 0o644))

	skills := DiscoverSkills(workDir)
	require.Len(t, skills, 1)
	require.Equal(t, "Project deploy skill", skills[0].Description)
}

func TestFormatSkillsSectionEscapesXML(t *testing.T) {
	got := formatSkillsSection([]Skill{{
		Name:        "x<y",
		Description: "Use & verify",
		Location:    "/tmp/a<b/SKILL.md",
	}})
	require.Contains(t, got, "<available_skills>")
	require.Contains(t, got, "<name>x&lt;y</name>")
	require.Contains(t, got, "<description>Use &amp; verify</description>")
	require.Contains(t, got, "<location>/tmp/a&lt;b/SKILL.md</location>")
	require.Contains(t, got, "Use the Read tool to load a skill's file")
}
