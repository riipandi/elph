package provider

import (
	"os"
	"path/filepath"
)

const (
	providersDirName   = "providers"
	providersDirEnv    = "ELPH_PROVIDERS_DIR"
	defaultElphHomeDir = ".elph"
)

// ProvidersDir returns the directory containing user-defined provider JSON files.
func ProvidersDir() (string, error) {
	if dir := os.Getenv(providersDirEnv); dir != "" {
		return dir, nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, defaultElphHomeDir, providersDirName), nil
}
