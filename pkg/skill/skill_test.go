package skill

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func writeSkill(t *testing.T, dir, content string) {
	t.Helper()
	require.NoError(t, os.MkdirAll(dir, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, FileName), []byte(content), 0o644))
}

func TestDiscoverAllIncludesDisableModelInvocation(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	hidden := filepath.Join(home, ".elph", "skills", "hidden")
	writeSkill(t, hidden, `---
name: hidden
description: Hidden skill
disable-model-invocation: true
---
# Hidden`)

	skills := DiscoverAll(workDir)
	require.Len(t, skills, 1)
	require.Equal(t, "hidden", skills[0].Name)
}

func TestDiscoverOmitsDisableModelInvocation(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	visible := filepath.Join(home, ".elph", "skills", "visible")
	writeSkill(t, visible, `---
name: visible
description: Visible skill
---
# Visible`)

	hidden := filepath.Join(home, ".elph", "skills", "hidden")
	writeSkill(t, hidden, `---
name: hidden
description: Hidden skill
disable-model-invocation: true
---
# Hidden`)

	skills := Discover(workDir)
	require.Len(t, skills, 1)
	require.Equal(t, "visible", skills[0].Name)
}

func TestResolveProjectOverridesGlobal(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	global := filepath.Join(home, ".elph", "skills", "deploy")
	writeSkill(t, global, `---
name: deploy
description: Global
type: inline
---
Global body`)

	project := filepath.Join(workDir, ".agents", "elph", "skills", "deploy")
	writeSkill(t, project, `---
name: deploy
description: Project
type: inline
---
Project body`)

	def, err := Resolve(workDir, "deploy")
	require.NoError(t, err)
	require.Equal(t, "Project body", def.Body)
}

func TestInvokeRejectsNonInlineType(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph", "skills", "manual")
	writeSkill(t, dir, `---
name: manual
description: Manual only
type: prompt
---
Body`)

	_, err := Invoke(context.Background(), workDir, "manual", "")
	require.Error(t, err)
	require.Contains(t, err.Error(), "inline")
}

func TestInvokeRejectsDisableModelInvocation(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph", "skills", "secret")
	writeSkill(t, dir, `---
name: secret
description: Secret
disableModelInvocation: true
type: inline
---
Body`)

	_, err := Invoke(context.Background(), workDir, "secret", "")
	require.Error(t, err)
	require.Contains(t, err.Error(), "disableModelInvocation")
}

func TestInvokeIncludesArgs(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	dir := filepath.Join(home, ".elph", "skills", "help")
	writeSkill(t, dir, `---
name: help
description: Help skill
type: inline
---
# Help`)

	out, err := Invoke(context.Background(), workDir, "help", "deploy prod")
	require.NoError(t, err)
	require.Contains(t, out, `<skill_content name="help">`)
	require.Contains(t, out, "# Help")
	require.Contains(t, out, "<user_args>\ndeploy prod\n</user_args>")
}

func TestEnterEnforcesMaxNestingDepth(t *testing.T) {
	ctx := WithDepthHolder(context.Background())
	require.NoError(t, Enter(ctx))
	require.NoError(t, Enter(ctx))
	require.NoError(t, Enter(ctx))
	require.Error(t, Enter(ctx))
}

func TestValidateName(t *testing.T) {
	require.NoError(t, ValidateName("code-review"))
	require.Error(t, ValidateName("PDF"))
	require.Error(t, ValidateName("-bad"))
	require.Error(t, ValidateName("bad--name"))
}
