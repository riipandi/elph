package runtime

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFormatToolDetailBodySuccess(t *testing.T) {
	got := FormatToolDetailBody("file contents", nil, false)
	require.Equal(t, "file contents", got)

	got = FormatToolDetailBody("", nil, false)
	require.Equal(t, "(no output)", got)

	got = FormatToolDetailBody("  \n", nil, false)
	require.Equal(t, "(no output)", got)
}

func TestFormatToolDetailBodyError(t *testing.T) {
	got := FormatToolDetailBody("", errors.New("file not found"), false)
	require.Equal(t, "Tool failed\n\nfile not found", got)

	got = FormatToolDetailBody("partial output", errors.New("permission denied"), false)
	require.Equal(t, "Tool failed\n\npermission denied\n\npartial output", got)
}

func TestFormatToolDetailBodyCancelled(t *testing.T) {
	got := FormatToolDetailBody("partial", nil, true)
	require.Equal(t, "partial\n(cancelled)", got)

	got = FormatToolDetailBody("", nil, true)
	require.Equal(t, "(cancelled)", got)
}

func TestFormatToolDisplay(t *testing.T) {
	got := FormatToolDisplay("Read", ToolResult{Output: "ok"})
	require.Contains(t, got, "Read")
	require.Contains(t, got, "ok")
}

func TestResolveToolRequestKnownBuiltin(t *testing.T) {
	got := ResolveToolRequest("websearch", map[string]string{"query": "cafe"})
	require.Equal(t, "WebSearch", got.Name)
	require.Equal(t, UnavailableNotExecutable, got.Reason)
	require.Contains(t, got.Body, "Tool unavailable")
	require.Contains(t, got.Body, "WebSearch")
	require.Contains(t, got.Body, "query: cafe")
}

func TestResolveToolRequestUnknown(t *testing.T) {
	got := ResolveToolRequest("mcp_search", nil)
	require.Equal(t, "Mcp_search", got.Name)
	require.Equal(t, UnavailableUnknown, got.Reason)
	require.Contains(t, got.Body, "Tool not available")
	require.Contains(t, got.Body, "Mcp_search")
}

func TestResolveToolRequestRequiresApproval(t *testing.T) {
	got := ResolveToolRequest("bash", map[string]string{"command": "rm -rf /"})
	require.Equal(t, "Bash", got.Name)
	require.Equal(t, UnavailableNotExecutable, got.Reason)
	require.Contains(t, got.Body, "requires approval")
	require.Contains(t, got.Body, "command: rm -rf /")
}

func TestResolveToolRequestDiagnostic(t *testing.T) {
	got := ResolveToolRequest("diagnostic_list_tools", nil)
	require.Equal(t, "diagnostic_list_tools", got.Name)
	require.Equal(t, UnavailableDiagnosticOnly, got.Reason)
	require.Contains(t, got.Body, "/diagnostic:list-tools")
}
