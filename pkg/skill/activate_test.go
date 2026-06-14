package skill

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatActivationStructuredWrapping(t *testing.T) {
	def := Definition{
		Name:        "pdf-tools",
		Description: "Work with PDFs",
		BaseDir:     "/tmp/pdf-tools",
		Body:        "# PDF\nExtract text from files.",
	}
	got := FormatActivation(def, "merge these")
	require.Contains(t, got, `<skill_content name="pdf-tools">`)
	require.Contains(t, got, "User-visible output must follow system prompt Output rules")
	require.Contains(t, got, "Extract text from files.")
	require.Contains(t, got, "Skill directory: /tmp/pdf-tools")
	require.Contains(t, got, "<user_args>\nmerge these\n</user_args>")
	require.Contains(t, got, "</skill_content>")
	require.NotContains(t, got, "skill: pdf-tools")
}

func TestFormatActivationListsBundledResources(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.MkdirAll(filepath.Join(dir, "scripts"), 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "scripts", "extract.py"), []byte("#"), 0o644))

	def := Definition{
		Name:    "pdf-tools",
		BaseDir: dir,
		Body:    "Run scripts/extract.py",
	}
	got := FormatActivation(def, "")
	require.Contains(t, got, "<skill_resources>")
	require.Contains(t, got, "<file>scripts/extract.py</file>")
}

func TestDiscoverIncludesAgentsSkillsConvention(t *testing.T) {
	home := t.TempDir()
	workDir := t.TempDir()
	t.Setenv("HOME", home)

	agentsDir := filepath.Join(home, ".agents", "skills", "shared")
	writeSkill(t, agentsDir, `---
name: shared
description: Cross-client skill
---
Shared body`)

	skills := DiscoverAll(workDir)
	require.Len(t, skills, 1)
	require.Equal(t, "shared", skills[0].Name)
	require.Contains(t, skills[0].Body, "Shared body")
}

func TestParseArgumentHintFrontmatter(t *testing.T) {
	meta, _, ok := parseFrontmatter(`---
name: identify
description: Identify the codebase
argument-hint: "<focus-area>"
---
Body`)
	require.True(t, ok)
	require.Equal(t, "<focus-area>", strings.TrimSpace(meta.ArgumentHint))
}

func TestParseDescriptionWithColon(t *testing.T) {
	meta, body, ok := parseFrontmatter(`---
name: pdf
description: Use this skill when: the user mentions PDFs
---
Body`)
	require.True(t, ok)
	require.Equal(t, "Use this skill when: the user mentions PDFs", strings.TrimSpace(meta.Description))
	require.Equal(t, "Body", strings.TrimSpace(body))
}
