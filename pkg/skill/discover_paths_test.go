package skill

import (
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestProjectAgentsSkillsOverridesUser(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkill(t, filepath.Join(home, ".agents", "skills", "deploy"), `---
name: deploy
description: User agents skill
---
User body`)

	writeSkill(t, filepath.Join(workDir, ".agents", "skills", "deploy"), `---
name: deploy
description: Project agents skill
---
Project body`)

	def, err := Resolve(workDir, "deploy")
	require.NoError(t, err)
	require.Equal(t, "Project body", def.Body)
}

func TestElphProjectOverridesAgentsProject(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	writeSkill(t, filepath.Join(workDir, ".agents", "skills", "lint"), `---
name: lint
description: Agents lint
---
Agents`)

	writeSkill(t, filepath.Join(workDir, ".agents", "elph", "skills", "lint"), `---
name: lint
description: Elph lint
---
Elph`)

	def, err := Resolve(workDir, "lint")
	require.NoError(t, err)
	require.Equal(t, "Elph", def.Body)
}