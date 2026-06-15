package jsoncfg

import (
	"io/fs"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestUnmarshalJSON(t *testing.T) {
	var out struct {
		Name string `json:"name"`
	}
	require.NoError(t, Unmarshal([]byte(`{"name":"elph"}`), &out))
	require.Equal(t, "elph", out.Name)
}

func TestUnmarshalJSONC(t *testing.T) {
	var out struct {
		Theme string `json:"theme"`
		Count int    `json:"count"`
	}
	raw := `{
		// theme preference
		"theme": "dark",
		"count": 1, /* trailing */
	}`
	require.NoError(t, Unmarshal([]byte(raw), &out))
	require.Equal(t, "dark", out.Theme)
	require.Equal(t, 1, out.Count)
}

func TestProviderID(t *testing.T) {
	id, ok := ProviderID("openai.json")
	require.True(t, ok)
	require.Equal(t, "openai", id)

	id, ok = ProviderID("custom.jsonc")
	require.True(t, ok)
	require.Equal(t, "custom", id)

	_, ok = ProviderID("notes.txt")
	require.False(t, ok)
}

func TestSelectProviderEntriesPrefersJSON(t *testing.T) {
	entries := []fs.DirEntry{
		dirEntry("openai.jsonc"),
		dirEntry("openai.json"),
		dirEntry("kimi.jsonc"),
	}

	selected, errs := SelectProviderEntries(entries)
	require.Empty(t, errs)
	require.Len(t, selected, 2)

	names := make([]string, len(selected))
	for i, entry := range selected {
		names[i] = entry.Name()
	}
	require.ElementsMatch(t, []string{"openai.json", "kimi.jsonc"}, names)
}

func TestResolveProviderPath(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "demo.jsonc"), []byte(`{}`), 0o644))

	path, err := ResolveProviderPath(dir, "demo")
	require.NoError(t, err)
	require.Equal(t, filepath.Join(dir, "demo.jsonc"), path)

	_, err = ResolveProviderPath(dir, "missing")
	require.Error(t, err)
}

type fakeDirEntry struct {
	name string
}

func (f fakeDirEntry) Name() string               { return f.name }
func (f fakeDirEntry) IsDir() bool                { return false }
func (f fakeDirEntry) Type() fs.FileMode            { return 0 }
func (f fakeDirEntry) Info() (fs.FileInfo, error) { return nil, nil }

func dirEntry(name string) fs.DirEntry { return fakeDirEntry{name: name} }