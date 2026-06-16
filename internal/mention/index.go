package mention

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
)

const maxIndexEntries = 4000

var skipDirNames = map[string]struct{}{
	".git":         {},
	".agents":      {},
	".elph":        {},
	".crush":       {},
	"node_modules": {},
	"vendor":       {},
	"dist":         {},
	"build":        {},
	"__pycache__":  {},
	".next":        {},
	".turbo":       {},
}

var errIndexLimit = errors.New("mention index limit reached")

// Index lists mentionable files and directories under root.
func Index(root string) ([]Entry, error) {
	root, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}

	entries := make([]Entry, 0, 256)
	err = filepath.WalkDir(root, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return nil
		}

		relPath, relErr := filepath.Rel(root, path)
		if relErr != nil || relPath == "." {
			return nil
		}
		relPath = filepath.ToSlash(relPath)

		if skip, skipDir := shouldSkip(relPath, d); skip {
			if skipDir && d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}

		entries = append(entries, Entry{Path: relPath, IsDir: d.IsDir()})
		if len(entries) >= maxIndexEntries {
			return errIndexLimit
		}
		return nil
	})
	if err != nil && !errors.Is(err, errIndexLimit) {
		return nil, err
	}

	slices.SortFunc(entries, func(a, b Entry) int {
		return strings.Compare(a.Path, b.Path)
	})
	return entries, nil
}

func shouldSkip(rel string, d fs.DirEntry) (skip bool, skipDir bool) {
	parts := strings.Split(rel, "/")
	name := parts[len(parts)-1]

	if strings.HasPrefix(name, ".") && name != "." {
		return true, d.IsDir()
	}
	if _, ok := skipDirNames[name]; ok {
		return true, d.IsDir()
	}
	return false, false
}

// IndexDir returns the indexed work directory, or "" when indexing is unavailable.
func IndexDir(root string) string {
	root, err := filepath.Abs(root)
	if err != nil {
		return ""
	}
	if _, err := os.Stat(root); err != nil {
		return ""
	}
	return root
}
