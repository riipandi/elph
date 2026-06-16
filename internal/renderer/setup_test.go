package renderer

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/riipandi/elph/internal/settings"
)

func TestMain(m *testing.M) {
	packageDir, err := os.Getwd()
	if err != nil {
		fmt.Fprintf(os.Stderr, "getwd failed: %v\n", err)
		os.Exit(1)
	}

	testWorkDir, err := os.MkdirTemp("", "elph-renderer-test-*")
	if err != nil {
		fmt.Fprintf(os.Stderr, "temp workdir failed: %v\n", err)
		os.Exit(1)
	}
	defer os.RemoveAll(testWorkDir)

	if chdirErr := os.Chdir(testWorkDir); chdirErr != nil {
		fmt.Fprintf(os.Stderr, "chdir to test workdir failed: %v\n", chdirErr)
		fmt.Fprintf(os.Stderr, "chdir to test workdir failed: %v\n", err)
		os.Exit(1)
	}

	// Remove stale session logs from older test runs that used packageDir as workDir.
	_ = os.RemoveAll(filepath.Join(packageDir, ".agents"))
	_ = os.RemoveAll(filepath.Join(packageDir, ".elph"))

	backup, err := backupSettings()
	if err != nil {
		fmt.Fprintf(os.Stderr, "settings backup failed: %v\n", err)
		os.Exit(1)
	}

	code := m.Run()

	if restoreErr := restoreSettings(backup); restoreErr != nil {
		fmt.Fprintf(os.Stderr, "settings restore failed: %v\n", restoreErr)
		os.Exit(1)
	}

	if chdirErr := os.Chdir(packageDir); chdirErr != nil {
		fmt.Fprintf(os.Stderr, "restore package dir failed: %v\n", chdirErr)
		fmt.Fprintf(os.Stderr, "restore package dir failed: %v\n", err)
		os.Exit(1)
	}

	os.Exit(code)
}

type settingsBackup struct {
	path string // real settings.json path
	data []byte // original content (nil if file didn't exist)
}

// backupSettings copies the real ~/.elph/settings.json into memory.
// If the file doesn't exist, backup.data is nil.
func backupSettings() (*settingsBackup, error) {
	path, err := settings.Path()
	if err != nil {
		return nil, fmt.Errorf("resolve settings path: %w", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return &settingsBackup{path: path, data: nil}, nil
		}
		return nil, fmt.Errorf("read settings %q: %w", path, err)
	}

	cp := make([]byte, len(data))
	copy(cp, data)
	return &settingsBackup{path: path, data: cp}, nil
}

// restoreSettings writes the backup data back to the original path.
// If the original file didn't exist, it removes the file so tests don't
// leave behind a settings.json they created.
func restoreSettings(b *settingsBackup) error {
	if b.data == nil {
		// Original file didn't exist — remove any file tests may have created.
		if err := os.Remove(b.path); err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("remove test-created settings %q: %w", b.path, err)
		}
		return nil
	}
	if err := os.WriteFile(b.path, b.data, 0o644); err != nil {
		return fmt.Errorf("restore settings %q: %w", b.path, err)
	}
	return nil
}
