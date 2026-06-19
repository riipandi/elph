package command

import (
	"testing"

	"github.com/riipandi/elph/internal/runtime/log"
	"github.com/riipandi/elph/internal/runtime/session"

	inttools "github.com/riipandi/elph/internal/tools"
	"github.com/riipandi/elph/pkg/ai/provider"
	"github.com/riipandi/elph/pkg/tools"
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

	dir := t.TempDir()
	t.Setenv("ELPH_PROVIDERS_DIR", dir)

	result := Execute("/model claude-sonnet", Context{})
	require.True(t, result.OK)
	require.True(t, result.OpenModelSelector)
	require.NotEmpty(t, result.SelectorCatalog.Providers)

	customDir := t.TempDir()
	writeProviderFile(t, customDir, "opencode.json", `{
		"baseUrl": "https://example.com/v1",
		"api": "openai-completions",
		"apiKey": "secret",
		"models": [{"id": "claude-sonnet", "name": "Claude Sonnet"}]
	}`)
	catalog, err := provider.LoadCatalog(customDir)
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
	require.Empty(t, result.Output)
	require.Equal(t, "Available tools", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, tools.Read)
	require.Contains(t, result.DetailBody, inttools.DiagnosticListTools)
}

func TestDiagnosticOpenLogSystem(t *testing.T) {
	dir := t.TempDir()
	session := session.NewSession(dir)
	require.NoError(t, log.AppendLog(session.LogPath, "user", "hello"))
	require.NoError(t, log.AppendLog(session.LogPath, "system", "notice"))

	result := Execute("/diagnostic:open-log system", Context{LogPath: session.LogPath})
	require.True(t, result.OK)
	require.Empty(t, result.Output)
	require.Equal(t, "Session log (system)", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, session.LogPath)
	require.Contains(t, result.DetailBody, "[system] notice")
	require.NotContains(t, result.DetailBody, "[user] hello")
}

func TestDiagnosticOpenLogRequests(t *testing.T) {
	dir := t.TempDir()
	session := session.NewSession(dir)
	require.NoError(t, log.AppendLog(session.RequestsLogPath, "requests", "POST /v1/messages"))

	result := Execute("/diagnostic:open-log requests", Context{RequestsLogPath: session.RequestsLogPath})
	require.True(t, result.OK)
	require.Empty(t, result.Output)
	require.Equal(t, "Requests log", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, session.RequestsLogPath)
	require.Contains(t, result.DetailBody, "POST /v1/messages")
}

func TestDiagnosticOpenLogRequestsEmptyFile(t *testing.T) {
	dir := t.TempDir()
	session := session.NewSession(dir)
	require.FileExists(t, session.RequestsLogPath)

	result := Execute("/diagnostic:open-log requests", Context{RequestsLogPath: session.RequestsLogPath})
	require.True(t, result.OK)
	require.Empty(t, result.Output)
	require.Equal(t, "Requests log", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, "is empty")
	require.Contains(t, result.DetailBody, session.RequestsLogPath)
}

func TestDiagnosticOpenLogThinkingDelta(t *testing.T) {
	dir := t.TempDir()
	session := session.NewSession(dir)
	require.NoError(t, log.AppendLog(session.RequestsLogPath, "thinking_delta", "step one"))

	result := Execute("/diagnostic:open-log thinking_delta", Context{RequestsLogPath: session.RequestsLogPath})
	require.True(t, result.OK)
	require.Empty(t, result.Output)
	require.Equal(t, "Requests log (thinking_delta)", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, "[thinking_delta] step one")
}

func TestDiagnosticOpenLogThinking(t *testing.T) {
	dir := t.TempDir()
	session := session.NewSession(dir)
	require.NoError(t, log.AppendLog(session.LogPath, "thinking", "step one"))
	require.NoError(t, log.AppendLog(session.LogPath, "ai", "answer"))

	result := Execute("/diagnostic:open-log thinking", Context{LogPath: session.LogPath})
	require.True(t, result.OK)
	require.Empty(t, result.Output)
	require.Equal(t, "Session log (thinking)", result.DetailLabel)
	require.True(t, result.DetailExpanded)
	require.Contains(t, result.DetailBody, "[thinking] step one")
	require.NotContains(t, result.DetailBody, "[ai] answer")
}

func TestDiagnosticOpenLogUsage(t *testing.T) {
	result := Execute("/diagnostic:open-log", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "Usage: /diagnostic:open-log <system | thinking | thinking_delta | ai | requests>")
}

func TestDiagnosticOpenLogUnknownArg(t *testing.T) {
	result := Execute("/diagnostic:open-log debug", Context{})
	require.True(t, result.OK)
	require.Contains(t, result.Output, "unknown log")
}
