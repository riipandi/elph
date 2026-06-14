package runtime

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

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
	require.False(t, result.Cancelled)
	if runtime.GOOS == "darwin" {
		require.Equal(t, sub, result.Output)
	} else {
		require.Contains(t, result.Output, "nested")
	}
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

func TestExecuteBashStreamsOutputChunks(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	var chunks []string
	result := ExecuteToolWithOutput(context.Background(), wd, tool.Bash, map[string]any{
		"command": "printf 'ab'; printf 'cd'",
	}, func(chunk string) {
		chunks = append(chunks, chunk)
	})
	require.NoError(t, result.Err)
	require.Equal(t, "abcd", result.Output)
	require.NotEmpty(t, chunks)
}

func TestExecuteBashTimesOut(t *testing.T) {
	t.Parallel()
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()

	wd := t.TempDir()
	result := ExecuteTool(ctx, wd, tool.Bash, map[string]any{
		"command": "sleep 5",
	})
	require.True(t, result.Cancelled)
}
