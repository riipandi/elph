package runtime

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestRunShellEcho(t *testing.T) {
	wd := t.TempDir()
	result := RunShell(wd, "echo hello")
	require.NoError(t, result.Err)
	require.Equal(t, 0, result.ExitCode)
	require.Equal(t, "hello", result.Output)
}

func TestRunShellUsesWorkDir(t *testing.T) {
	wd := t.TempDir()
	sub := filepath.Join(wd, "nested")
	require.NoError(t, os.MkdirAll(sub, 0o755))

	result := RunShell(sub, "pwd")
	require.NoError(t, result.Err)
	require.Equal(t, 0, result.ExitCode)
	if runtime.GOOS == "darwin" {
		require.Equal(t, sub, result.Output)
	} else {
		require.Contains(t, result.Output, "nested")
	}
}

func TestRunShellNonZeroExit(t *testing.T) {
	wd := t.TempDir()
	result := RunShell(wd, "exit 7")
	require.NoError(t, result.Err)
	require.Equal(t, 7, result.ExitCode)
}

func TestFormatShellContext(t *testing.T) {
	got := FormatShellContext("ls", "a\nb", 0)
	require.Contains(t, got, "Ran `ls`")
	require.Contains(t, got, "```")
	require.Contains(t, got, "a\nb")

	got = FormatShellContext("true", "", 0)
	require.Contains(t, got, "(no output)")

	got = FormatShellContext("false", "", 1)
	require.Contains(t, got, "(exit 1)")
}

func TestFormatShellDisplay(t *testing.T) {
	got := FormatShellDisplay("ls", "out", 0, nil, false)
	require.Equal(t, "$ ls\nout", got)

	got = FormatShellDisplay("false", "", 2, nil, false)
	require.Contains(t, got, "(exit 2)")

	got = FormatShellDisplay("sleep", "partial", 0, nil, true)
	require.Contains(t, got, "partial")
	require.Contains(t, got, "(cancelled)")
}

func TestRunShellContextCancellation(t *testing.T) {
	wd := t.TempDir()
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan ShellResult, 1)
	go func() {
		done <- RunShellContext(ctx, wd, "sleep 30", nil)
	}()
	time.Sleep(50 * time.Millisecond)
	cancel()
	result := <-done
	require.True(t, result.Cancelled)
}

func TestTruncateShellOutput(t *testing.T) {
	long := strings.Repeat("x", defaultMaxShellBytes+100)
	got := truncateShellOutput(long)
	require.True(t, strings.HasPrefix(got, "... (output truncated)"))
	require.LessOrEqual(t, len(got), defaultMaxShellBytes+len("... (output truncated)\n")+10)
}
