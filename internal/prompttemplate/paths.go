package prompttemplate

import (
	"os"
	"path/filepath"
)

const (
	promptsDirName     = "prompts"
	defaultElphHomeDir = ".elph"
	promptsDirEnv      = "ELPH_PROMPTS_DIR"
)

// GlobalDir returns ~/.elph/prompts (or ELPH_PROMPTS_DIR when set).
func GlobalDir() (string, error) {
	if dir := os.Getenv(promptsDirEnv); dir != "" {
		return dir, nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, defaultElphHomeDir, promptsDirName), nil
}

// ProjectDir returns <workDir>/.elph/prompts.
func ProjectDir(workDir string) string {
	return filepath.Join(workDir, defaultElphHomeDir, promptsDirName)
}