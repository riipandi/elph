package renderer

import (
	"fmt"
	"os"
	"testing"

	"github.com/riipandi/elph/internal/settings"
)

func TestMain(m *testing.M) {
	backup, err := backupSettings()
	if err != nil {
		fmt.Fprintf(os.Stderr, "settings backup failed: %v\n", err)
		os.Exit(1)
	}

	code := m.Run()

	if err := restoreSettings(backup); err != nil {
		fmt.Fprintf(os.Stderr, "settings restore failed: %v\n", err)
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
