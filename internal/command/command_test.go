package command

import (
	"testing"

	"github.com/riipandi/elph/internal/runtime"
	"github.com/riipandi/elph/internal/tools"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestExecuteHelp(t *testing.T) {
	result := Execute("/help", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "/help")
	require.Contains(t, result.Output, "/changelog")
	require.Contains(t, result.Output, "/diagnostic:list-tools")
	require.Contains(t, result.Output, "/exit")
}

func TestExecuteExit(t *testing.T) {
	result := Execute("/exit", Context{})
	require.True(t, result.OK)
	require.True(t, result.Quit)
}

func TestExecuteQuitAlias(t *testing.T) {
	result := Execute("/quit", Context{})
	require.True(t, result.OK)
	require.True(t, result.Quit)
}

func TestExecuteWithArgs(t *testing.T) {
	t.Setenv("ELPH_PROVIDERS_DIR", t.TempDir())

	result := Execute("/model claude-sonnet", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "no providers found")

	dir := t.TempDir()
	writeProviderFile(t, dir, "opencode.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [{"id": "claude-sonnet", "name": "Claude Sonnet"}]
	}`)
	catalog, err := provider.LoadCatalog(dir)
	require.NoError(t, err)

	result = Execute("/model claude", Context{Catalog: catalog})
	require.True(t, result.OK)
	require.True(t, result.OpenModelSelector)
	require.Equal(t, "claude", result.SelectorQuery)
}

func TestExecuteAlias(t *testing.T) {
	result := Execute("/config", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "/settings")
}

func TestExecuteUnknown(t *testing.T) {
	result := Execute("/nope", Context{})
	require.False(t, result.OK)
	require.Contains(t, result.Output, "Unknown command")
	require.Contains(t, result.Output, "/help")
}

func TestParseCommand(t *testing.T) {
	name, args := parse("  /model  sonnet  ")
	require.Equal(t, "model", name)
	require.Equal(t, "sonnet", args)
}

func TestAllIncludesReferencedCommands(t *testing.T) {
	names := make([]string, len(All(Context{})))
	for i, cmd := range All(Context{}) {
		names[i] = cmd.Name
	}
	require.Contains(t, names, "help")
	require.Contains(t, names, "changelog")
	require.Contains(t, names, DiagnosticListTools)
}

func TestDiagnosticListTools(t *testing.T) {
	result := Execute("/diagnostic:list-tools", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, tool.Read)
	require.Contains(t, result.Output, tools.ListTools)
}

func TestDiagnosticSystemPrompt(t *testing.T) {
	result := Execute("/diagnostic:system-prompt", Context{
		SystemPrompt: "You are an expert coding assistant.",
	})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "You are an expert coding assistant.")
}

func TestDiagnosticSystemPromptEmpty(t *testing.T) {
	result := Execute("/diagnostic:system-prompt", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "not yet implemented")
}

func TestDiagnosticOpenLogSystem(t *testing.T) {
	dir := t.TempDir()
	session := runtime.NewSession(dir)
	require.NoError(t, runtime.AppendLog(session.LogPath, "user", "hello"))
	require.NoError(t, runtime.AppendLog(session.LogPath, "system", "notice"))

	result := Execute("/diagnostic:open-log system", Context{LogPath: session.LogPath})
	require.True(t, result.OK)
	require.Contains(t, result.Output, session.LogPath)
	require.Contains(t, result.Output, "[system] notice")
	require.NotContains(t, result.Output, "[user] hello")
}

func TestDiagnosticOpenLogRequests(t *testing.T) {
	dir := t.TempDir()
	session := runtime.NewSession(dir)
	require.NoError(t, runtime.AppendLog(session.RequestsLogPath, "requests", "POST /v1/messages"))

	result := Execute("/diagnostic:open-log requests", Context{RequestsLogPath: session.RequestsLogPath})
	require.True(t, result.OK)
	require.Contains(t, result.Output, session.RequestsLogPath)
	require.Contains(t, result.Output, "POST /v1/messages")
}

func TestDiagnosticOpenLogUsage(t *testing.T) {
	result := Execute("/diagnostic:open-log", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "Usage: /diagnostic:open-log <requests | system>")
}

func TestDiagnosticOpenLogUnknownArg(t *testing.T) {
	result := Execute("/diagnostic:open-log debug", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "unknown log")
}
