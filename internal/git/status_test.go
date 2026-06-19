package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestReadNonRepo(t *testing.T) {
	dir := t.TempDir()
	st := Read(dir)
	require.Equal(t, "—", st.Branch)
	require.False(t, st.IsRepo)
}

func TestReadBranchAndDiffStats(t *testing.T) {
	dir := t.TempDir()
	initRepoWithChanges(t, dir)

	st := Read(dir)
	require.True(t, st.IsRepo)
	require.Equal(t, "master", st.Branch)
	require.Positive(t, st.Added)
}

func initRepoWithChanges(t *testing.T, dir string) {
	t.Helper()

	git := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		out, err := cmd.CombinedOutput()
		require.NoError(t, err, "git %v failed: %s", args, string(out))
	}

	git("init", "--initial-branch=master")
	git("config", "user.email", "test@example.com")
	git("config", "user.name", "Test")

	require.NoError(t, os.WriteFile(filepath.Join(dir, "a.txt"), []byte("hello\n"), 0o644))
	git("add", "a.txt")
	git("commit", "-m", "init")

	require.NoError(t, os.WriteFile(filepath.Join(dir, "a.txt"), []byte("hello\nworld\n"), 0o644))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "b.txt"), []byte("new\n"), 0o644))
	git("add", "b.txt")
}

func TestCountChangedFiles(t *testing.T) {
	require.Equal(t, 2, countChangedFiles("?M a.txt\nM  b.txt\n?? c.txt"))
	require.Equal(t, 0, countChangedFiles("?? a.txt"))
	require.Equal(t, 0, countChangedFiles(""))
}
