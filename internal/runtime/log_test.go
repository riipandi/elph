package runtime

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/riipandi/elph/internal/projectdir"
	"github.com/stretchr/testify/require"
	"go.jetify.com/typeid/v2"
)

func TestOpenAppendAndReadLog(t *testing.T) {
	dir := t.TempDir()
	id := typeid.MustGenerate("sess")

	path, err := OpenLog(dir, id)
	require.NoError(t, err)
	require.FileExists(t, path)
	require.Contains(t, path, filepath.Join(".agents", "elph", "logs", id.String(), eventsLogName))
	require.FileExists(t, filepath.Join(projectdir.Root(dir), ".gitignore"))

	require.NoError(t, AppendLog(path, "user", "hello"))
	require.NoError(t, AppendLog(path, "system", "world"))

	content, err := ReadLogTail(path, 4096)
	require.NoError(t, err)
	require.Contains(t, content, "[user] hello")
	require.Contains(t, content, "[system] world")

	raw, err := os.ReadFile(path)
	require.NoError(t, err)
	lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
	require.Len(t, lines, 2)
	var rec map[string]any
	require.NoError(t, json.Unmarshal([]byte(lines[0]), &rec))
	require.Equal(t, "user", rec["kind"])
	require.Equal(t, "hello", rec["msg"])
}

func TestFilterLogByKind(t *testing.T) {
	dir := t.TempDir()
	id := typeid.MustGenerate("sess")

	path, err := OpenLog(dir, id)
	require.NoError(t, err)
	require.NoError(t, AppendLog(path, "user", "hello"))
	require.NoError(t, AppendLog(path, "system", "notice"))

	content, err := FilterLogByKind(path, "system", 4096)
	require.NoError(t, err)
	require.Contains(t, content, "[system] notice")
	require.NotContains(t, content, "[user] hello")
}

func TestOpenRequestsLogCreatesFile(t *testing.T) {
	dir := t.TempDir()
	id := typeid.MustGenerate("sess")

	path, err := OpenRequestsLog(dir, id)
	require.NoError(t, err)
	require.FileExists(t, path)
	require.Contains(t, path, requestsLogName)
}

func TestRequestsLogPath(t *testing.T) {
	id := typeid.MustGenerate("sess")
	got := RequestsLogPath("/tmp/project", id)
	require.Contains(t, got, ".agents/elph/logs/")
	require.Contains(t, got, id.String())
	require.Contains(t, got, requestsLogName)
}

func TestReadLogTailTruncates(t *testing.T) {
	dir := t.TempDir()
	id := typeid.MustGenerate("sess")
	path, err := OpenLog(dir, id)
	require.NoError(t, err)

	for i := 0; i < 200; i++ {
		require.NoError(t, AppendLog(path, "system", strings.Repeat("x", 40)))
	}

	content, err := ReadLogTail(path, 200)
	require.NoError(t, err)
	require.LessOrEqual(t, len(content), 400)
}
