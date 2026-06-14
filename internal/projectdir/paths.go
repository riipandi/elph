package projectdir

import (
	"os"
	"path/filepath"
	"strings"
)

const (
	// RelRoot is the project-local Elph directory relative to the workspace root.
	RelRoot = ".agents/elph"

	gitignoreName = ".gitignore"
	gitignoreBody = "" +
		"# Elph agent runtime (logs, local settings, MCP config)\n" +
		".gitignore\n" +
		"logs/\n" +
		"settings.json\n" +
		"settings/\n" +
		"mcp/\n" +
		"attachments/\n"
)

var gitignoreRequiredEntries = []string{
	".gitignore",
	"logs/",
	"settings.json",
	"settings/",
	"mcp/",
	"attachments/",
}

// Root returns <workDir>/.agents/elph.
func Root(workDir string) string {
	return filepath.Join(workDir, ".agents", "elph")
}

// PromptsDir returns <workDir>/.agents/elph/prompts.
func PromptsDir(workDir string) string {
	return filepath.Join(Root(workDir), "prompts")
}

// SkillsDir returns <workDir>/.agents/elph/skills.
func SkillsDir(workDir string) string {
	return filepath.Join(Root(workDir), "skills")
}

// LogsDir returns <workDir>/.agents/elph/logs.
func LogsDir(workDir string) string {
	return filepath.Join(Root(workDir), "logs")
}

// SessionDir returns <workDir>/.agents/elph/logs/<sessionID>.
func SessionDir(workDir, sessionID string) string {
	return filepath.Join(LogsDir(workDir), sessionID)
}

// AttachmentsDir returns <workDir>/.agents/elph/attachments.
func AttachmentsDir(workDir string) string {
	return filepath.Join(Root(workDir), "attachments")
}

// EnsureRoot creates <workDir>/.agents/elph and writes .gitignore when missing.
func EnsureRoot(workDir string) error {
	root := Root(workDir)
	if err := os.MkdirAll(root, 0o755); err != nil {
		return err
	}
	return ensureGitignore(root)
}

func ensureGitignore(root string) error {
	path := filepath.Join(root, gitignoreName)
	raw, err := os.ReadFile(path)
	if err == nil {
		if gitignoreUpToDate(string(raw)) {
			return nil
		}
	} else if !os.IsNotExist(err) {
		return err
	}
	return os.WriteFile(path, []byte(gitignoreBody), 0o644)
}

func gitignoreUpToDate(content string) bool {
	for _, entry := range gitignoreRequiredEntries {
		if !strings.Contains(content, entry) {
			return false
		}
	}
	return true
}
