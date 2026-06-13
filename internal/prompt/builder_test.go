package prompt

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestBuildIncludesBaseTemplate(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "You are an expert coding assistant.")
	require.Contains(t, got, "## Output")
	require.Contains(t, got, "## Git")
}

func TestBuildIncludesDynamicTools(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Available Tools")
	require.Contains(t, got, "### File Tools")
	require.Contains(t, got, "**Read** (auto-allow): Read a text file's contents")
	require.Contains(t, got, "**Bash** (requires-approval): Execute a shell command")
	require.Contains(t, got, "**ExitPlanMode** (auto-allow): Exit Plan mode and submit the plan")
	require.Contains(t, got, "Requires user confirmation after completion.")
	require.Contains(t, got, "### Diagnostic Tools")
	require.Contains(t, got, "**diagnostic_list_tools** (auto-allow)")
	require.Contains(t, got, "**diagnostic_system_prompt** (auto-allow)")
	require.Contains(t, got, "**diagnostic_open_log** (auto-allow)")
}

func TestBuildRespectsToolFilter(t *testing.T) {
	read, ok := tool.Get(tool.Read)
	require.True(t, ok)

	got := Build(Options{Tools: []tool.Definition{read}})
	require.Contains(t, got, "**Read** (auto-allow)")
	require.NotContains(t, got, "**Bash**")
	require.Contains(t, got, "**diagnostic_list_tools** (auto-allow)")
}

func TestBuildIncludesAgentsMD(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte("Use Go 1.26.\n"), 0o644))

	got := Build(Options{WorkDir: dir})
	require.Contains(t, got, "## Project Instructions")
	require.Contains(t, got, "AGENTS.md")
	require.Contains(t, got, "Use Go 1.26.")
}

func TestBuildIncludesAdditionalInstructions(t *testing.T) {
	got := Build(Options{AdditionalInstructions: "Prefer table-driven tests."})
	require.Contains(t, got, "## Additional Instructions")
	require.Contains(t, got, "Prefer table-driven tests.")
}

func TestBuildSectionOrder(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte("project rule"), 0o644))

	got := Build(Options{
		WorkDir:                dir,
		AdditionalInstructions: "user rule",
	})

	base := strings.Index(got, "You are an expert coding assistant.")
	tools := strings.Index(got, "## Available Tools")
	agents := strings.Index(got, "## Project Instructions")
	extra := strings.Index(got, "## Additional Instructions")

	require.Less(t, base, tools)
	require.Less(t, tools, agents)
	require.Less(t, agents, extra)
}