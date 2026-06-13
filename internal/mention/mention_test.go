package mention

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestFindActiveMention(t *testing.T) {
	input := "fix @internal/ren"
	query, start, ok := FindActive(input, len(input))
	require.True(t, ok)
	require.Equal(t, 4, start)
	require.Equal(t, "internal/ren", query)
}

func TestFindActiveRejectsEmailLikeToken(t *testing.T) {
	_, _, ok := FindActive("user@host.com", 13)
	require.False(t, ok)
}

func TestMatchSuggestionIndex(t *testing.T) {
	entries := []Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "internal", IsDir: true},
	}
	idx, ok := MatchSuggestionIndex(entries, "internal/renderer/input.go")
	require.True(t, ok)
	require.Equal(t, 0, idx)
}

func TestCompleteInsertsDirectorySlash(t *testing.T) {
	got := Complete("see @int", 4, 8, Entry{Path: "internal", IsDir: true})
	require.Equal(t, "see @internal/", got)
}

func TestIndexSkipsHiddenAndVendorDirs(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.MkdirAll(filepath.Join(dir, "internal", "app"), 0o755))
	require.NoError(t, os.MkdirAll(filepath.Join(dir, ".git"), 0o755))
	require.NoError(t, os.MkdirAll(filepath.Join(dir, "node_modules", "pkg"), 0o755))
	require.NoError(t, os.WriteFile(filepath.Join(dir, "internal", "app", "main.go"), []byte("pkg"), 0o644))

	entries, err := Index(dir)
	require.NoError(t, err)
	paths := make([]string, len(entries))
	for i, entry := range entries {
		paths[i] = entry.Path
	}
	require.Contains(t, paths, "internal")
	require.Contains(t, paths, "internal/app")
	require.Contains(t, paths, "internal/app/main.go")
	require.NotContains(t, paths, ".git")
	require.NotContains(t, paths, "node_modules")
}

func TestSuggestPrefersPathMatch(t *testing.T) {
	entries := []Entry{
		{Path: "internal/renderer/input.go"},
		{Path: "pkg/tool/tool.go"},
	}
	got := Suggest("input", entries)
	require.NotEmpty(t, got)
	require.Equal(t, "internal/renderer/input.go", got[0].Path)
}
