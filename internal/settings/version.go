package settings

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/riipandi/elph/pkg/jsoncfg"
)

const versionFileName = "version.json"

// Dummy release metadata until update checks are implemented.
const (
	dummyStableVersion    = "0.2.16"
	dummyReleaseCheckedAt = "2026-06-02T05:19:25.373305Z"
)

// VersionFile is persisted at ~/.elph/version.json.
type VersionFile struct {
	LastSyncProviders string `json:"lastSyncProviders,omitempty"`
	ReleaseCheckedAt  string `json:"relaseCheckedAt,omitempty"`
	StableVersion     string `json:"stableVersion,omitempty"`
	Version           string `json:"version,omitempty"`
}

// VersionPath returns ~/.elph/version.json.
func VersionPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, defaultElphHomeDir, versionFileName), nil
}

// LoadVersion reads version metadata from disk. Missing files return defaults.
func LoadVersion() (VersionFile, error) {
	path, err := VersionPath()
	if err != nil {
		return VersionFile{}, err
	}

	raw, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			v := defaultVersionFile()
			if migrateErr := v.migrateLastSyncFromSettings(); migrateErr != nil {
				return VersionFile{}, err
			}
			return v, nil
		}
		return VersionFile{}, fmt.Errorf("read version %q: %w", path, err)
	}

	var v VersionFile
	if err := jsoncfg.Unmarshal(raw, &v); err != nil {
		return VersionFile{}, fmt.Errorf("decode version %q: %w", path, err)
	}
	v = v.withDefaults()
	if err := v.migrateLastSyncFromSettings(); err != nil {
		return VersionFile{}, err
	}
	return v, nil
}

// SaveVersion writes version metadata to ~/.elph/version.json.
func SaveVersion(v VersionFile) error {
	path, err := VersionPath()
	if err != nil {
		return err
	}
	if verMkdirErr := os.MkdirAll(filepath.Dir(path), 0o755); verMkdirErr != nil {
		return fmt.Errorf("create version dir: %w", err)
	}

	v = v.withDefaults()
	payload, err := json.MarshalIndent(v, "", "\t")
	if err != nil {
		return fmt.Errorf("encode version: %w", err)
	}
	payload = append(payload, '\n')

	if err := os.WriteFile(path, payload, 0o600); err != nil {
		return fmt.Errorf("write version %q: %w", path, err)
	}
	return nil
}

func defaultVersionFile() VersionFile {
	return VersionFile{
		ReleaseCheckedAt: dummyReleaseCheckedAt,
		StableVersion:    dummyStableVersion,
		Version:          dummyStableVersion,
	}
}

func (v VersionFile) withDefaults() VersionFile {
	if v.ReleaseCheckedAt == "" {
		v.ReleaseCheckedAt = dummyReleaseCheckedAt
	}
	if v.StableVersion == "" {
		v.StableVersion = dummyStableVersion
	}
	if v.Version == "" {
		v.Version = dummyStableVersion
	}
	return v
}

func (v *VersionFile) migrateLastSyncFromSettings() error {
	if v.LastSyncProviders != "" {
		return nil
	}
	cfg, err := Load()
	if err != nil {
		return err
	}
	var legacy string
	if cfg.Models != nil {
		legacy = cfg.Models.legacyLastSync()
	}
	if legacy == "" {
		return nil
	}
	v.LastSyncProviders = legacy
	return SaveVersion(*v)
}

// LastSyncProvidersTime parses lastSyncProviders (RFC3339 or RFC3339Nano).
func (v VersionFile) LastSyncProvidersTime() (time.Time, bool) {
	if v.LastSyncProviders == "" {
		return time.Time{}, false
	}
	if t, err := time.Parse(time.RFC3339Nano, v.LastSyncProviders); err == nil {
		return t, true
	}
	t, err := time.Parse(time.RFC3339, v.LastSyncProviders)
	if err != nil {
		return time.Time{}, false
	}
	return t, true
}

// MarkProvidersSynced records a successful provider metadata sync at now.
func MarkProvidersSynced(now time.Time) error {
	v, err := LoadVersion()
	if err != nil {
		return err
	}
	v.LastSyncProviders = now.UTC().Format(time.RFC3339Nano)
	return SaveVersion(v)
}
