package prompt

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/tools"
	"github.com/stretchr/testify/require"
)

func TestBuildIncludesBaseTemplate(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "You are an expert AI coding assistant, operate in Elph CLI.")
	require.Contains(t, got, "## Output")
	require.Contains(t, got, "## Git")
	require.Contains(t, got, "## Asking the User")
	require.Contains(t, got, "required gate")
	require.Contains(t, got, "wait for their response, then proceed")
}

func TestBuildAskUserSectionInjectedWithCustomSystemPrompt(t *testing.T) {
	got := Build(Options{
		SystemPrompt: "Custom agent.\n\n{{.AvailableTools}}",
	})
	require.Contains(t, got, "Custom agent.")
	require.Contains(t, got, "required gate")
	require.Contains(t, got, "call AskUser first")
	require.NotContains(t, got, "You are an expert AI coding assistant, operate in Elph CLI.")
}

func TestBuildIncludesDynamicTools(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Available Tools")
	require.Contains(t, got, "### File Tools")
	require.Contains(t, got, "File tools handle reading, writing, and searching the local filesystem")
	require.Contains(t, got, "- Read (auto-allow): Read a text file. Fails on directories")
	require.Contains(t, got, "- Grep (auto-allow):")
	require.Contains(t, got, "- Glob (auto-allow):")
	require.Contains(t, got, "### Shell Tools")
	require.Contains(t, got, "- Bash (requires-approval):")
	require.Contains(t, got, "### Web Tools")
	require.Contains(t, got, "- WebSearch (auto-allow): Web search with multiple engines")
	require.Contains(t, got, "### State Management")
	require.Contains(t, got, "- TodoList (auto-allow): Manage a task to-do list")
	require.Contains(t, got, "### Collaboration Tools")
	require.Contains(t, got, "- AskUser (auto-allow):")
	require.Contains(t, got, "### Diagnostic Tools")
	require.Contains(t, got, "- DiagnosticListTools (auto-allow): List all tools currently available to the agent")
	require.Contains(t, got, "- DiagnosticSystemPrompt (auto-allow):")
	require.Contains(t, got, "- DiagnosticOpenLog (auto-allow):")
}

func TestBuildRespectsToolFilter(t *testing.T) {
	read, ok := tools.Get(tools.Read)
	require.True(t, ok)

	got := Build(Options{Tools: []Entry{EntryFromBuiltin(read)}})
	require.Contains(t, got, "- Read (auto-allow): Read a text file. Fails on directories")
	require.NotContains(t, got, "- Bash (")
	require.Contains(t, got, "- DiagnosticListTools (auto-allow):")
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
	agentsPath := filepath.Join(dir, "AGENTS.md")
	require.NoError(t, os.WriteFile(agentsPath, []byte("Use Go 1.26.\n"), 0o644))

	got := Build(Options{WorkDir: dir, CurrentDate: "2026-06-15", AgentMode: "build"})
	require.Contains(t, got, "<project_context>")
	require.Contains(t, got, "Project-specific instructions and guidelines:")
	require.Contains(t, got, `<project_instructions path="`+agentsPath+`">`)
	require.Contains(t, got, "Use Go 1.26.")
	require.Contains(t, got, "</project_context>")
}

func TestBuildIncludesResponseLanguageInheritDefault(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Response Language")
	require.Contains(t, got, "Detect the language of each user message and write your replies in that same language.")
	require.Contains(t, got, "If the user explicitly asks you to respond in a different language")
	require.NotContains(t, got, "Write user-facing replies in English by default.")
}

func TestBuildIncludesResponseLanguageInheritExplicit(t *testing.T) {
	got := Build(Options{PreferedResponseLanguage: "inherit"})
	require.Contains(t, got, "Detect the language of each user message and write your replies in that same language.")
}

func TestBuildIncludesResponseLanguageFixed(t *testing.T) {
	got := Build(Options{PreferedResponseLanguage: "Indonesian"})
	require.Contains(t, got, "Write user-facing replies in Indonesian by default.")
	require.NotContains(t, got, "Detect the language of each user message")
}

func TestBuildIncludesSkillsSection(t *testing.T) {
	got := Build(Options{
		Skills: []Skill{{
			Name:        "review",
			Description: "Review local changes",
			Location:    "/tmp/review/SKILL.md",
		}},
	})
	require.Contains(t, got, "<available_skills>")
	require.Contains(t, got, "<name>review</name>")
	require.Contains(t, got, "<description>Review local changes</description>")
	require.Contains(t, got, "<location>/tmp/review/SKILL.md</location>")
	require.Contains(t, got, "overrides skill templates and examples")
}

func TestBuildIncludesSkillOutputRules(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Skill Output")
	require.Contains(t, got, "Apply loaded skill instructions for workflow and reasoning only.")
	require.Contains(t, got, "These rules override skill templates, examples, and labels.")
	require.Contains(t, got, "omit skill names, mode labels (e.g. ASIDE:")
}

func TestBuildIncludesRuntimeAndSessionState(t *testing.T) {
	dir := t.TempDir()
	got := Build(Options{
		WorkDir:     dir,
		CurrentDate: "2026-06-15",
		AgentMode:   "plan",
	})

	absDir, err := filepath.Abs(dir)
	require.NoError(t, err)

	require.Contains(t, got, "Current date: 2026-06-15")
	require.Contains(t, got, "Current working directory: "+absDir)
	require.Contains(t, got, "<session_mode>plan</session_mode>")
}

func TestBuildIncludesAdditionalInstructions(t *testing.T) {
	got := Build(Options{AdditionalInstructions: "Prefer table-driven tests."})
	require.Contains(t, got, "## Additional Instructions")
	require.Contains(t, got, "Prefer table-driven tests.")
}

func TestBuildThinkingSectionSpacing(t *testing.T) {
	got := Build(Options{})
	require.Contains(t, got, "## Asking the User")
	require.Contains(t, got, "tool definitions, and session assumptions.\n\n## Asking the User")
	require.Contains(t, got, "short and specific.\n\nYou can use <think> tags")
}

func TestBuildCompactSpacing(t *testing.T) {
	got := Build(Options{})

	require.NotContains(t, got, "\n\n\n")
	require.NotContains(t, got, "\n\n\n")
	require.Contains(t, got, "writing files.\n\n## Output")
	require.Contains(t, got, "show paths clearly.\n\n## File Operations")
	require.Contains(t, got, "Glob first.\n\n## Available Tools")
	require.Contains(t, got, "## Available Tools\n\n### File Tools")
	require.Contains(t, got, "### Shell Tools")
	require.Contains(t, got, "### State Management")
	require.Contains(t, got, "- TodoList (auto-allow): Manage a task to-do list")
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
		CurrentDate:            "2026-06-15",
		AgentMode:              "build",
	})

	base := strings.Index(got, "You are an expert AI coding assistant, operate in Elph CLI.")
	tools := strings.Index(got, "## Available Tools")
	project := strings.Index(got, "<project_context>")
	runtime := strings.Index(got, "Current date: 2026-06-15")
	session := strings.Index(got, "<session_state>")
	guardrails := strings.Index(got, "## Guardrails")
	responseLang := strings.Index(got, "## Response Language")
	extra := strings.Index(got, "## Additional Instructions")

	require.Less(t, base, tools)
	require.Less(t, tools, project)
	require.Less(t, project, runtime)
	require.Less(t, runtime, session)
	require.Less(t, session, guardrails)
	require.Less(t, guardrails, responseLang)
	require.Less(t, responseLang, extra)

	askUser := strings.Index(got, "## Asking the User")
	require.Greater(t, askUser, guardrails)
	require.Less(t, askUser, responseLang)
}
