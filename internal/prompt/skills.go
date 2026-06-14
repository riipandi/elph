package prompt

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/riipandi/elph/internal/projectdir"
	"gopkg.in/yaml.v3"
)

const (
	skillsDirName   = "skills"
	skillsDirEnv    = "ELPH_SKILLS_DIR"
	skillFileName   = "SKILL.md"
	defaultElphHome = ".elph"
)

// Skill describes a discoverable SKILL.md entry for the system prompt.
type Skill struct {
	Name        string
	Description string
	Location    string
}

// DiscoverSkills loads skills from ~/.elph/skills and <workDir>/.agents/elph/skills.
// Project skills override global skills with the same name.
func DiscoverSkills(workDir string) []Skill {
	byName := make(map[string]Skill)
	order := make([]string, 0)

	if globalDir, err := globalSkillsDir(); err == nil {
		for _, skill := range loadSkillsFromDir(globalDir) {
			if _, exists := byName[skill.Name]; !exists {
				order = append(order, skill.Name)
			}
			byName[skill.Name] = skill
		}
	}

	for _, skill := range loadSkillsFromDir(projectSkillsDir(workDir)) {
		if _, exists := byName[skill.Name]; !exists {
			order = append(order, skill.Name)
		}
		byName[skill.Name] = skill
	}

	out := make([]Skill, 0, len(order))
	for _, name := range order {
		out = append(out, byName[name])
	}
	return out
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

func projectSkillsDir(workDir string) string {
	return projectdir.SkillsDir(workDir)
}

func loadSkillsFromDir(dir string) []Skill {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}

	out := make([]Skill, 0)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		path := filepath.Join(dir, entry.Name(), skillFileName)
		if skill, ok := loadSkillFile(path, entry.Name()); ok {
			out = append(out, skill)
		}
	}
	return out
}

func loadSkillFile(path, fallbackName string) (Skill, bool) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return Skill{}, false
	}

	meta, body, _ := parseSkillFrontmatter(string(raw))
	name := strings.TrimSpace(meta.Name)
	if name == "" {
		name = fallbackName
	}

	description := strings.TrimSpace(meta.Description)
	if description == "" {
		description = firstNonEmptyLine(body)
	}
	if description == "" {
		return Skill{}, false
	}

	abs, err := filepath.Abs(path)
	if err != nil {
		abs = path
	}

	return Skill{
		Name:        name,
		Description: description,
		Location:    abs,
	}, true
}

type skillFrontmatter struct {
	Name        string `yaml:"name"`
	Description string `yaml:"description"`
}

func parseSkillFrontmatter(raw string) (skillFrontmatter, string, bool) {
	raw = strings.TrimPrefix(raw, "\ufeff")
	trimmed := strings.TrimLeft(raw, " \t")
	if !strings.HasPrefix(trimmed, "---") {
		return skillFrontmatter{}, raw, false
	}

	rest := trimmed[3:]
	if len(rest) > 0 && rest[0] == '\n' {
		rest = rest[1:]
	} else if len(rest) > 0 && rest[0] == '\r' {
		if len(rest) > 1 && rest[1] == '\n' {
			rest = rest[2:]
		} else {
			rest = rest[1:]
		}
	}

	end := strings.Index(rest, "\n---")
	if end < 0 {
		return skillFrontmatter{}, raw, false
	}

	meta := rest[:end]
	body := rest[end+4:]
	body = strings.TrimPrefix(body, "\r\n")
	body = strings.TrimPrefix(body, "\n")

	var parsed skillFrontmatter
	if err := yaml.Unmarshal([]byte(meta), &parsed); err != nil {
		return skillFrontmatter{}, raw, false
	}
	return parsed, body, true
}

func firstNonEmptyLine(body string) string {
	for _, line := range strings.Split(body, "\n") {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		return trimmed
	}
	return ""
}

func formatSkillsSection(skills []Skill) string {
	if len(skills) == 0 {
		return ""
	}

	var b strings.Builder
	b.WriteString("The following skills provide specialized instructions for specific tasks.\n")
	b.WriteString("Use the Read tool to load a skill's file when the task matches its description.\n")
	b.WriteString("When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.\n\n")
	b.WriteString("<available_skills>\n")
	for _, skill := range skills {
		b.WriteString("  <skill>\n")
		b.WriteString("    <name>")
		b.WriteString(escapeXML(skill.Name))
		b.WriteString("</name>\n")
		b.WriteString("    <description>")
		b.WriteString(escapeXML(collapseWhitespace(skill.Description)))
		b.WriteString("</description>\n")
		b.WriteString("    <location>")
		b.WriteString(escapeXML(skill.Location))
		b.WriteString("</location>\n")
		b.WriteString("  </skill>\n")
	}
	b.WriteString("</available_skills>")
	return b.String()
}

func collapseWhitespace(s string) string {
	return strings.Join(strings.Fields(s), " ")
}

func escapeXML(s string) string {
	s = strings.ReplaceAll(s, "&", "&amp;")
	s = strings.ReplaceAll(s, "<", "&lt;")
	s = strings.ReplaceAll(s, ">", "&gt;")
	return s
}
