package skill

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
)

const (
	skillsDirName   = "skills"
	skillsDirEnv    = "ELPH_SKILLS_DIR"
	defaultElphHome = ".elph"
)

// Discover loads skills for the system prompt and Skill tool.
// Skills with disableModelInvocation are omitted (use slash commands instead).
func Discover(workDir string) []Definition {
	return discoverMerged(workDir, false)
}

// DiscoverAll loads every skill for slash commands (/skill:<name>), including
// disableModelInvocation entries.
func DiscoverAll(workDir string) []Definition {
	return discoverMerged(workDir, true)
}

func discoverMerged(workDir string, includeModelHidden bool) []Definition {
	byName, order := mergeDefinitions(workDir)

	out := make([]Definition, 0, len(order))
	for _, name := range order {
		def := byName[name]
		if !includeModelHidden && def.DisableModelInvocation {
			continue
		}
		out = append(out, def)
	}
	return out
}

func mergeDefinitions(workDir string) (map[string]Definition, []string) {
	byName := make(map[string]Definition)
	order := make([]string, 0)

	for _, dir := range discoveryScopes(workDir) {
		for _, def := range loadFromDir(dir) {
			if prev, exists := byName[def.Name]; exists {
				if prev.Location != "" && def.Location != "" && prev.Location != def.Location {
					slog.Default().Debug(
						"skill name collision; later scope wins",
						"name", def.Name,
						"shadowed", prev.Location,
						"winner", def.Location,
					)
				}
			} else {
				order = append(order, def.Name)
			}
			byName[def.Name] = def
		}
	}
	return byName, order
}

// Resolve finds a skill by name for Skill tool invocation and slash commands.
func Resolve(workDir, rawName string) (Definition, error) {
	name := SanitizeName(rawName)
	if name == "" {
		return Definition{}, fmt.Errorf("empty skill name")
	}

	byName, _ := mergeDefinitions(workDir)
	def, ok := byName[name]
	if !ok {
		return Definition{}, fmt.Errorf("unknown skill: %s", rawName)
	}
	return def, nil
}

func globalSkillsDir() (string, error) {
	if dir := strings.TrimSpace(os.Getenv(skillsDirEnv)); dir != "" {
		return dir, nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, defaultElphHome, skillsDirName), nil
}

func loadFromDir(dir string) []Definition {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}

	out := make([]Definition, 0)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		name := entry.Name()
		if strings.HasPrefix(name, ".") || name == "node_modules" {
			continue
		}
		path := filepath.Join(dir, name, FileName)
		if def, ok := loadFile(path, name); ok {
			out = append(out, def)
		}
	}
	return out
}

func loadFile(path, dirName string) (Definition, bool) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return Definition{}, false
	}

	meta, body, ok := parseFrontmatter(string(raw))
	if !ok {
		slog.Default().Warn("skipping skill with unparseable frontmatter", "path", path)
		return Definition{}, false
	}

	name := strings.TrimSpace(meta.Name)
	if name == "" {
		name = dirName
	}
	name = SanitizeName(name)

	if warn := ValidateName(name); warn != nil {
		slog.Default().Warn("skill name is not spec-compliant", "path", path, "name", name, "err", warn)
	}

	if SanitizeName(dirName) != "" && name != SanitizeName(dirName) {
		slog.Default().Warn("skill name does not match parent directory", "path", path, "name", name, "dir", dirName)
	}

	description := strings.TrimSpace(meta.Description)
	if description == "" {
		description = firstNonEmptyLine(body)
	}
	if description == "" {
		slog.Default().Warn("skipping skill without description", "path", path)
		return Definition{}, false
	}
	if warn := ValidateDescription(description); warn != nil {
		slog.Default().Warn("skill description is not spec-compliant", "path", path, "err", warn)
	}

	abs, err := filepath.Abs(path)
	if err != nil {
		abs = path
	}

	return Definition{
		Name:                   name,
		Description:            description,
		ArgumentHint:           strings.TrimSpace(meta.ArgumentHint),
		Location:               abs,
		BaseDir:                filepath.Dir(abs),
		Type:                   normalizeType(meta.Type),
		DisableModelInvocation: meta.DisableModelInvocation,
		Body:                   strings.TrimSpace(body),
	}, true
}
