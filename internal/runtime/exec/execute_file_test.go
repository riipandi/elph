package exec

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/riipandi/elph/pkg/tools"
	"github.com/stretchr/testify/require"
)

func requireRipgrep(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("rg"); err != nil {
		t.Skip("rg not installed")
	}
}

func TestExecuteRead(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "hello.txt")
	require.NoError(t, os.WriteFile(path, []byte("hello\nworld"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Read, map[string]any{
		"path": "hello.txt",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "1\thello")
	require.Contains(t, result.Output, "2\tworld")
	require.Contains(t, result.Output, "Total lines in file: 2")
}

func TestExecuteReadTruncatesLargeFile(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "big.txt")
	// File with enough rows that line-numbered output exceeds maxReadBytes
	var sb strings.Builder
	for i := 0; i < 10000; i++ {
		sb.WriteString("this is line number " + fmt.Sprint(i) + "\n")
	}
	require.NoError(t, os.WriteFile(path, []byte(sb.String()), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Read, map[string]any{
		"path": "big.txt",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "(output truncated)")
	require.LessOrEqual(t, len(result.Output), maxReadBytes+256)
}

func TestExecuteWriteCreatesFileAndParents(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()

	result := ExecuteTool(context.Background(), wd, tools.Write, map[string]any{
		"path":     "nested/out.txt",
		"contents": "created",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "Wrote 7 bytes")

	data, err := os.ReadFile(filepath.Join(wd, "nested", "out.txt"))
	require.NoError(t, err)
	require.Equal(t, "created", string(data))
}

func TestExecuteWriteOverwritesAndAllowsEmptyContents(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "file.txt")
	require.NoError(t, os.WriteFile(path, []byte("old"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Write, map[string]any{
		"path":     "file.txt",
		"contents": "",
	})
	require.NoError(t, result.Err)

	data, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Empty(t, string(data))
}

func TestExecuteEditReplacesSingleOccurrence(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "main.go")
	require.NoError(t, os.WriteFile(path, []byte("foo bar foo"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Edit, map[string]any{
		"path":       "main.go",
		"old_string": "foo bar",
		"new_string": "baz qux",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "Replaced 1 occurrence")

	data, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Equal(t, "baz qux foo", string(data))
}

func TestExecuteEditReplaceAll(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "main.go")
	require.NoError(t, os.WriteFile(path, []byte("foo bar foo"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Edit, map[string]any{
		"path":        "main.go",
		"old_string":  "foo",
		"new_string":  "baz",
		"replace_all": true,
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "Replaced 2 occurrence")

	data, err := os.ReadFile(path)
	require.NoError(t, err)
	require.Equal(t, "baz bar baz", string(data))
}

func TestExecuteEditErrors(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	path := filepath.Join(wd, "main.go")
	require.NoError(t, os.WriteFile(path, []byte("alpha beta alpha"), 0o644))

	missing := ExecuteTool(context.Background(), wd, tools.Edit, map[string]any{
		"path":       "main.go",
		"old_string": "missing",
		"new_string": "x",
	})
	require.Error(t, missing.Err)
	require.Contains(t, missing.Err.Error(), "not found")

	ambiguous := ExecuteTool(context.Background(), wd, tools.Edit, map[string]any{
		"path":       "main.go",
		"old_string": "alpha",
		"new_string": "x",
	})
	require.Error(t, ambiguous.Err)
	require.Contains(t, ambiguous.Err.Error(), "appears 2 times")
}

func TestExecuteGrepFindsMatch(t *testing.T) {
	t.Parallel()
	requireRipgrep(t)

	wd := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(wd, "a.go"), []byte("package main\nfunc main() {}\n"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Grep, map[string]any{
		"pattern": "func main",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "a.go")
}

func TestExecuteGrepNoMatches(t *testing.T) {
	t.Parallel()
	requireRipgrep(t)

	wd := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(wd, "a.go"), []byte("package main\n"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Grep, map[string]any{
		"pattern": "not-in-file",
	})
	require.NoError(t, result.Err)
	require.Equal(t, "(no matches)", result.Output)
}

func TestExecuteGrepFilesWithMatches(t *testing.T) {
	t.Parallel()
	requireRipgrep(t)

	wd := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(wd, "a.go"), []byte("needle"), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(wd, "b.go"), []byte("other"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Grep, map[string]any{
		"pattern":     "needle",
		"output_mode": "files_with_matches",
	})
	require.NoError(t, result.Err)
	require.Contains(t, result.Output, "a.go")
	require.NotContains(t, result.Output, "b.go")
}

func TestExecuteGlobSimpleAndRecursive(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	require.NoError(t, os.MkdirAll(filepath.Join(wd, "pkg", "app"), 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(wd, "root.go"), []byte("x"), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(wd, "pkg", "app", "main.go"), []byte("x"), 0o644))

	simple := ExecuteTool(context.Background(), wd, tools.Glob, map[string]any{
		"pattern": "*.go",
	})
	require.NoError(t, simple.Err)
	require.Contains(t, simple.Output, "root.go")
	require.NotContains(t, simple.Output, "main.go")

	recursive := ExecuteTool(context.Background(), wd, tools.Glob, map[string]any{
		"pattern": "**/*.go",
	})
	require.NoError(t, recursive.Err)
	require.Contains(t, recursive.Output, "root.go")
	require.Contains(t, recursive.Output, "main.go")

	scoped := ExecuteTool(context.Background(), wd, tools.Glob, map[string]any{
		"pattern": "pkg/**/*.go",
	})
	require.NoError(t, scoped.Err)
	require.NotContains(t, scoped.Output, "root.go")
	require.Contains(t, scoped.Output, "main.go")
}

func TestExecuteReadOnDirectory(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	sub := filepath.Join(wd, "subdir")
	require.NoError(t, os.MkdirAll(sub, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(sub, "file.txt"), []byte("content"), 0o644))

	result := ExecuteTool(context.Background(), wd, tools.Read, map[string]any{
		"path": "subdir",
	})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "is a directory")
	require.Contains(t, result.Err.Error(), "Glob")
}

func TestExecuteEditOnDirectory(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	sub := filepath.Join(wd, "subdir")
	require.NoError(t, os.MkdirAll(sub, 0o755))

	result := ExecuteTool(context.Background(), wd, tools.Edit, map[string]any{
		"path":       "subdir",
		"old_string": "x",
		"new_string": "y",
	})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "is a directory")
}

func TestExecuteWriteOnDirectory(t *testing.T) {
	t.Parallel()
	wd := t.TempDir()
	sub := filepath.Join(wd, "subdir")
	require.NoError(t, os.MkdirAll(sub, 0o755))

	result := ExecuteTool(context.Background(), wd, tools.Write, map[string]any{
		"path":     "subdir",
		"contents": "hello",
	})
	require.Error(t, result.Err)
	require.Contains(t, result.Err.Error(), "directory")
}
