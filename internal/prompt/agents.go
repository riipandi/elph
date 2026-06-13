package prompt

import (
	"os"
	"path/filepath"
	"strings"
)

const agentsFileName = "AGENTS.md"

// FindAgentsMD walks up from startDir and returns the nearest AGENTS.md content and path.
func FindAgentsMD(startDir string) (content string, path string, ok bool) {
	dir, err := filepath.Abs(startDir)
	if err != nil || dir == "" {
		return "", "", false
	}

	for {
		candidate := filepath.Join(dir, agentsFileName)
		data, err := os.ReadFile(candidate)
		if err == nil {
			text := strings.TrimSpace(string(data))
			if text != "" {
				return text, candidate, true
			}
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return "", "", false
		}
		dir = parent
	}
}