package provider

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"

	"github.com/riipandi/elph/pkg/jsoncfg"
)

// LoadCatalog reads all provider JSON files from dir (or ~/.elph/providers when empty).
func LoadCatalog(dir string) (Catalog, error) {
	if dir == "" {
		var err error
		dir, err = ProvidersDir()
		if err != nil {
			return Catalog{}, err
		}
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return Catalog{}, nil
		}
		return Catalog{}, fmt.Errorf("read providers dir %q: %w", dir, err)
	}

	catalog := Catalog{Dir: dir}
	providerEntries, selectErrs := jsoncfg.SelectProviderEntries(entries)
	catalog.Errors = append(catalog.Errors, selectErrs...)
	for _, entry := range providerEntries {
		provider, err := loadProviderFile(dir, entry)
		if err != nil {
			catalog.Errors = append(catalog.Errors, err)
			continue
		}
		catalog.Providers = append(catalog.Providers, provider)
	}
	return catalog, nil
}

func loadProviderFile(dir string, entry fs.DirEntry) (RegisteredProvider, error) {
	id, ok := jsoncfg.ProviderID(entry.Name())
	if !ok || id == "" {
		return RegisteredProvider{}, fmt.Errorf("invalid provider filename %q", entry.Name())
	}

	path := filepath.Join(dir, entry.Name())
	raw, err := os.ReadFile(path)
	if err != nil {
		return RegisteredProvider{}, fmt.Errorf("provider %q: %w", id, err)
	}

	var cfg FileConfig
	if err := jsoncfg.Unmarshal(raw, &cfg); err != nil {
		return RegisteredProvider{}, fmt.Errorf("provider %q: decode: %w", id, err)
	}

	cfg = ApplyGatewayThinkingCompat(id, cfg)
	return normalizeProvider(id, cfg)
}
