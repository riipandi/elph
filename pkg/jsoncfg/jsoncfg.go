// Package jsoncfg parses standard JSON and JSONC (comments and trailing commas).
package jsoncfg

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"github.com/tidwall/jsonc"
)

const (
	ExtJSON  = ".json"
	ExtJSONC = ".jsonc"
)

// Unmarshal decodes JSON or JSONC into v.
func Unmarshal(data []byte, v any) error {
	clean := jsonc.ToJSON(data)
	if err := json.Unmarshal(clean, v); err != nil {
		return err
	}
	return nil
}

// ProviderID returns the provider id from a config filename.
func ProviderID(name string) (string, bool) {
	base := strings.TrimSpace(name)
	if base == "" {
		return "", false
	}
	ext := strings.ToLower(filepath.Ext(base))
	switch ext {
	case ExtJSON, ExtJSONC:
		id := strings.TrimSuffix(base, ext)
		if id == "" {
			return "", false
		}
		return id, true
	default:
		return "", false
	}
}

// IsProviderConfig reports whether name is a provider config filename.
func IsProviderConfig(name string) bool {
	_, ok := ProviderID(name)
	return ok
}

type providerPick struct {
	entry fs.DirEntry
	ext   string
}

// SelectProviderEntries returns provider config files, preferring .json over .jsonc
// when both exist for the same provider id.
func SelectProviderEntries(entries []fs.DirEntry) ([]fs.DirEntry, []error) {
	byID := make(map[string]providerPick)
	var errs []error

	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		id, ok := ProviderID(entry.Name())
		if !ok {
			continue
		}
		ext := strings.ToLower(filepath.Ext(entry.Name()))
		existing, found := byID[id]
		if !found {
			byID[id] = providerPick{entry: entry, ext: ext}
			continue
		}
		switch {
		case existing.ext == ExtJSON && ext == ExtJSONC:
			continue
		case existing.ext == ExtJSONC && ext == ExtJSON:
			byID[id] = providerPick{entry: entry, ext: ext}
		default:
			errs = append(errs, fmt.Errorf("duplicate provider %q: %s and %s", id, existing.entry.Name(), entry.Name()))
		}
	}

	out := make([]fs.DirEntry, 0, len(byID))
	for _, pick := range byID {
		out = append(out, pick.entry)
	}
	return out, errs
}

// ResolveProviderPath returns the path to an existing provider config file.
func ResolveProviderPath(dir, providerID string) (string, error) {
	providerID = strings.TrimSpace(providerID)
	if providerID == "" {
		return "", fmt.Errorf("provider id is required")
	}
	for _, ext := range []string{ExtJSON, ExtJSONC} {
		path := filepath.Join(dir, providerID+ext)
		if _, err := os.Stat(path); err == nil {
			return path, nil
		}
	}
	return "", fmt.Errorf("provider %q not found", providerID)
}
