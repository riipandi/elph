package skill

import (
	"os"
	"path/filepath"
)

// discoveryScopes returns skill roots in ascending precedence. Later scopes
// override earlier ones when skill names collide (project over user).
func discoveryScopes(workDir string) []string {
	var scopes []string

	if home, err := os.UserHomeDir(); err == nil {
		scopes = append(scopes,
			filepath.Join(home, ".agents", "skills"),
			filepath.Join(home, ".claude", "skills"),
		)
	}
	if global, err := globalSkillsDir(); err == nil {
		scopes = append(scopes, global)
	}
	if workDir != "" {
		scopes = append(scopes,
			filepath.Join(workDir, ".agents", "skills"),
			filepath.Join(workDir, ".agents", "elph", "skills"),
		)
	}
	return scopes
}
