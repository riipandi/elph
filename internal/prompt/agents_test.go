package prompt

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFindAgentsMDInWorkDir(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "AGENTS.md")
	require.NoError(t, os.WriteFile(path, []byte("root agents"), 0o644))

	content, found, ok := FindAgentsMD(dir)
	require.True(t, ok)
	require.Equal(t, path, found)
	require.Equal(t, "root agents", content)
}

func TestFindAgentsMDWalksUpTree(t *testing.T) {
	root := t.TempDir()
	nested := filepath.Join(root, "pkg", "app")
	require.NoError(t, os.MkdirAll(nested, 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(root, "AGENTS.md"), []byte("monorepo rule"), 0o644))

	content, found, ok := FindAgentsMD(nested)
	require.True(t, ok)
	require.Equal(t, filepath.Join(root, "AGENTS.md"), found)
	require.Equal(t, "monorepo rule", content)
}

func TestFindAgentsMDMissing(t *testing.T) {
	_, _, ok := FindAgentsMD(t.TempDir())
	require.False(t, ok)
}