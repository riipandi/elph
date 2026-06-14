package prompttemplate

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestLoadFromGlobalAndProjectDirs(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("ELPH_PROMPTS_DIR", filepath.Join(home, ".elph", "prompts"))

	workDir := t.TempDir()
	projectPrompts := ProjectDir(workDir)
	require.NoError(t, os.MkdirAll(projectPrompts, 0o755))

	globalDir, err := GlobalDir()
	require.NoError(t, err)
	require.NoError(t, os.MkdirAll(globalDir, 0o755))
	require.NoError(t, err)
	require.NoError(t, os.WriteFile(filepath.Join(globalDir, "identify.md"), []byte(`---
description: Identify the codebase
argument-hint: "<focus>"
---
Identify the architecture with focus on $1.`), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(projectPrompts, "review.md"), []byte(`Review staged changes.`), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(projectPrompts, "identify.md"), []byte(`Project-specific identify prompt.`), 0o644))

	got := Load(workDir)
	require.Len(t, got, 2)

	byName := make(map[string]Template, len(got))
	for _, tmpl := range got {
		byName[tmpl.Name] = tmpl
	}

	require.Equal(t, "project", byName["identify"].Scope)
	require.Equal(t, "Project-specific identify prompt.", byName["identify"].Content)
	require.Equal(t, "project", byName["review"].Scope)
	require.Equal(t, "Review staged changes.", byName["review"].Content)
}

func TestExpandSubstitutesArguments(t *testing.T) {
	templates := []Template{{
		Name:    "identify",
		Content: "Identify $1 and $@",
	}}
	expanded, ok := Expand("/identify auth \"token flow\"", templates)
	require.True(t, ok)
	require.Equal(t, `Identify auth and auth token flow`, expanded)
}