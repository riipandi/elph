package provider

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/riipandi/elph/pkg/jsoncfg"
)

// ConfigEnabled reports whether an optional enabled flag is active.
// Missing or nil means enabled.
func ConfigEnabled(enabled *bool) bool {
	return enabled == nil || *enabled
}

// ProviderConfigEnabled reports whether a provider file is enabled.
func ProviderConfigEnabled(cfg FileConfig) bool {
	return ConfigEnabled(cfg.Enabled)
}

// ModelConfigEnabled reports whether a model entry is enabled.
func ModelConfigEnabled(model ModelConfig) bool {
	return ConfigEnabled(model.Enabled)
}

// SetProviderEnabled toggles a provider file's enabled flag.
func SetProviderEnabled(providerID string, enabled bool) error {
	return updateProviderConfig(providerID, func(cfg *FileConfig) error {
		cfg.Enabled = boolPtr(enabled)
		return nil
	})
}

// SetModelEnabled toggles one model's enabled flag inside a provider file.
func SetModelEnabled(providerID, modelID string, enabled bool) error {
	return updateProviderConfig(providerID, func(cfg *FileConfig) error {
		for i := range cfg.Models {
			if cfg.Models[i].ID == modelID {
				cfg.Models[i].Enabled = boolPtr(enabled)
				return nil
			}
		}
		return fmt.Errorf("provider %q: model %q not found", providerID, modelID)
	})
}

func updateProviderConfig(providerID string, mutate func(*FileConfig) error) error {
	providerID = normalizeProviderID(providerID)
	if providerID == "" {
		return fmt.Errorf("provider id is required")
	}

	dir, err := ProvidersDir()
	if err != nil {
		return err
	}

	path, err := jsoncfg.ResolveProviderPath(dir, providerID)
	if err != nil {
		return err
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("read provider %q: %w", providerID, err)
	}

	var cfg FileConfig
	if enableErr := jsoncfg.Unmarshal(raw, &cfg); enableErr != nil {
		return fmt.Errorf("decode provider %q: %w", providerID, err)
	}

	if enableErr := mutate(&cfg); enableErr != nil {
		return err
	}

	//nolint:gosec // intentionally writing API key to config file
	payload, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return fmt.Errorf("encode provider %q: %w", providerID, err)
	}
	payload = append(payload, '\n')

	if err := os.WriteFile(path, payload, 0o600); err != nil {
		return fmt.Errorf("write provider %q: %w", providerID, err)
	}
	return nil
}

func normalizeProviderID(id string) string {
	id = filepath.Base(id)
	if id == "." || id == string(filepath.Separator) {
		return ""
	}
	ext := strings.ToLower(filepath.Ext(id))
	switch ext {
	case jsoncfg.ExtJSON, jsoncfg.ExtJSONC:
		id = id[:len(id)-len(ext)]
	}
	return id
}

func boolPtr(v bool) *bool {
	return &v
}
