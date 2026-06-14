package runtime

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/pkg/tool"
	"github.com/stretchr/testify/require"
)

func TestExecuteBashEcho(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	result := ExecuteTool(context.Background(), wd, tool.Bash, map[string]any{
		"command": "echo hello",
	})
	require.NoError(t, result.Err)
	require.False(t, result.Cancelled)
	require.Equal(t, "hello", result.Output)
}

func TestExecuteBashNonZeroExit(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	result := ExecuteTool(context.Background(), wd, tool.Bash, map[string]any{
		"command": "exit 3",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "(exit 3)")
}

func TestExecuteBashUsesWorkDir(t *testing.T) {
	t.Parallel()
	root := t.TempDir()
	sub := filepath.Join(root, "nested")
	require.NoError(t, os.MkdirAll(sub, 0o755))

	result := ExecuteTool(context.Background(), sub, tool.Bash, map[string]any{
		"command": "pwd",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "nested")
}

func TestExecuteBashMissingCommand(t *testing.T) {
	t.Parallel()
	result := ExecuteTool(context.Background(), t.TempDir(), tool.Bash, map[string]any{})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "command")
}

func TestExecuteBashInvalidSyntax(t *testing.T) {
	t.Parallel()
	result := ExecuteTool(context.Background(), t.TempDir(), tool.Bash, map[string]any{
		"command": "if then",
	})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "invalid shell syntax")
}

func TestValidateShellCommandRejectsNullByte(t *testing.T) {
	t.Parallel()
	require.Error(t, validateShellCommand("echo\x00bad"))
}
