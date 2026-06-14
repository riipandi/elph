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
	require.Contains(t, got, "You are an expert AI coding assistant, operate in Elph CLI.")
	require.Contains(t, got, "## Output")
	require.Contains(t, got, "## Git")
	require.Contains(t, got, "Use `AskUser` only when a decision has meaningful trade-offs")
}

func TestBuildIncludesDynamicTools(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Available Tools")
	require.Contains(t, got, "### File Tools")
	require.Contains(t, got, "File tools handle reading, writing, and searching the local filesystem")
	require.Contains(t, got, "- Read (auto-allow): Read a text file's contents")
	require.Contains(t, got, "- Grep (auto-allow):")
	require.Contains(t, got, "- Glob (auto-allow):")
	require.Contains(t, got, "### Shell Tools")
	require.Contains(t, got, "- Bash (requires-approval):")
	require.Contains(t, got, "### Collaboration Tools")
	require.Contains(t, got, "- AskUser (auto-allow):")
	require.NotContains(t, got, "- WebSearch (")
	require.Contains(t, got, "### Diagnostic Tools")
	require.Contains(t, got, "- diagnostic_list_tools (auto-allow): List all tools currently available to the agent")
	require.Contains(t, got, "- diagnostic_system_prompt (auto-allow):")
	require.Contains(t, got, "- diagnostic_open_log (auto-allow):")
}

func TestBuildRespectsToolFilter(t *testing.T) {
	read, ok := tool.Get(tool.Read)
	require.True(t, ok)

	got := Build(Options{Tools: []Entry{EntryFromBuiltin(read)}})
	require.Contains(t, got, "- Read (auto-allow): Read a text file's contents")
	require.NotContains(t, got, "- Bash (")
	require.Contains(t, got, "- diagnostic_list_tools (auto-allow):")
}

func TestBuildIncludesExternalTools(t *testing.T) {
	got := Build(Options{
		Tools: []Entry{
			ExternalEntry("get_design_context", "MCP: figma", "auto-allow", "Generate UI code from Figma designs"),
		},
	})

	require.Contains(t, got, "### MCP: figma")
	require.Contains(t, got, "- get_design_context (auto-allow): Generate UI code from Figma designs")
}

func TestBuildFormatsAlwaysApprovePermission(t *testing.T) {
	got := Build(Options{
		Tools: []Entry{
			ExternalEntry("safe_read", "MCP: docs", "always-approve", "Read-only document lookup"),
		},
	})

	require.Contains(t, got, "- safe_read (always-approve): Read-only document lookup")
}

func TestBuildCustomSystemPrompt(t *testing.T) {
	got := Build(Options{
		SystemPrompt: "Custom agent.\n\n## Tools\n\n{{.AvailableTools}}",
	})

	require.Contains(t, got, "Custom agent.")
	require.Contains(t, got, "## Available Tools")
	require.Contains(t, got, "- Read (auto-allow):")
	require.NotContains(t, got, "You are an expert coding assistant.")
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

func TestBuildThinkingSectionSpacing(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "tool definitions, and session assumptions.\n\nYou can use <think> tags")
}

func TestBuildCompactSpacing(t *testing.T) {
	got := Build(Options{})

	require.NotContains(t, got, "\n\n\n")
	require.Contains(t, got, "writing files.\n\n## Output")
	require.Contains(t, got, "show paths clearly.\n\n## Available Tools")
	require.Contains(t, got, "## Available Tools\n\n### File Tools")
	require.Contains(t, got, "### Shell Tools")
	require.Contains(t, got, "### Collaboration Tools")
	require.NotContains(t, got, "The following tools are currently available:")
	require.NotContains(t, got, "| Tool | Default Approval | Description |")
}

func TestBuildSectionOrder(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte("project rule"), 0o644))

	got := Build(Options{
		WorkDir:                dir,
		AdditionalInstructions: "user rule",
	})

	base := strings.Index(got, "You are an expert AI coding assistant, operate in Elph CLI.")
	tools := strings.Index(got, "## Available Tools")
	guardrails := strings.Index(got, "## Guardrails")
	agents := strings.Index(got, "## Project Instructions")
	extra := strings.Index(got, "## Additional Instructions")

	require.Less(t, base, tools)
	require.Less(t, tools, guardrails)
	require.Less(t, guardrails, agents)
	require.Less(t, agents, extra)
}
